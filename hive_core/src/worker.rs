//! # Worker - Concurrent Task Executor with Tokio
//!
//! Worker implementa ejecución concurrente de tareas:
//! - Múltiples workers running en paralelo ( Tokio spawn)
//! - Claim tasks del Blackboard automáticamente
//! - Genera código real con LLM
//! - Crea ramas Git y commits reales

use crate::blackboard::{Blackboard, FileAction, FileChange, Specialist, Task, TaskResult};
use crate::brain::Brain;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval, sleep};
use uuid::Uuid;

/// Un Worker individual - thread-safe para Tokio
pub struct Worker {
    id: Uuid,
    specialist: Specialist,
    blackboard: Arc<Blackboard>,
    brain: Arc<Brain>,
    target_dir: std::path::PathBuf,
    max_retries: u8,
}

impl Worker {
    pub fn new(
        specialist: Specialist,
        blackboard: Arc<Blackboard>,
        brain: Arc<Brain>,
        target_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            specialist: specialist.clone(),
            blackboard,
            brain,
            target_dir,
            max_retries: 3,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Run el worker - ciclo infinito de procesar tareas
    pub async fn run(&self) {
        tracing::info!(
            worker_id = %self.id,
            specialist = ?self.specialist,
            "Worker started"
        );

        let mut tick = interval(Duration::from_secs(2));

        loop {
            tick.tick().await;

            // Try to claim a task
            match self.blackboard.claim_task(self.id, &self.specialist).await {
                Ok(Some(task)) => {
                    if let Err(e) = self.process_task(&task).await {
                        tracing::error!(error = %e, "Task processing failed");
                    }
                }
                Ok(None) => {
                    // No task available, continue waiting
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to claim task");
                }
            }
        }
    }

    /// Procesa una tarea individual
    async fn process_task(&self, task: &Task) -> Result<()> {
        tracing::info!(
            task_id = %task.id,
            file = %task.target_file.display(),
            "Processing task"
        );

        let mut retry_count = 0;
        let mut last_error = String::new();

        while retry_count < self.max_retries {
            match self.execute_task(task).await {
                Ok(result) => {
                    self.blackboard.complete_task(result).await?;
                    return Ok(());
                }
                Err(e) => {
                    last_error = e.to_string();
                    retry_count += 1;
                    tracing::warn!(
                        task_id = %task.id,
                        attempt = retry_count,
                        error = %e,
                        "Task failed, retrying"
                    );
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }

        // All retries failed
        let result = TaskResult {
            task_id: task.id,
            worker_id: self.id,
            success: false,
            changes: vec![],
            llm_prompt: None,
            llm_response: None,
            error: Some(last_error),
        };
        self.blackboard.complete_task(result).await?;

        Err(anyhow::anyhow!(
            "Task failed after {} retries",
            self.max_retries
        ))
    }

    /// Ejecuta la tarea - genera código y lo aplica
    async fn execute_task(&self, task: &Task) -> Result<TaskResult> {
        // Get context from blackboard
        let context = self.blackboard.get_context(task).await;

        // Generate code with LLM
        let code = self.brain.generate_code_for_task(task, &context).await?;

        // Parse the LLM response into file changes
        let changes = self.parse_code_response(&code, task)?;

        // Apply changes to filesystem
        for change in &changes {
            self.apply_file_change(change).await?;
        }

        // Create git branch and commit using external git commands
        let branch_name = format!("hive/worker/{:?}-{}", self.specialist, task.id);

        self.git_add_and_commit(&changes, task, &branch_name)?;

        // Create local MR
        let mr_id = Uuid::new_v4();

        tracing::info!(
            task_id = %task.id,
            branch = %branch_name,
            mr_id = %mr_id,
            files_changed = changes.len(),
            "Task executed successfully"
        );

        Ok(TaskResult {
            task_id: task.id,
            worker_id: self.id,
            success: true,
            changes,
            llm_prompt: Some(format!("Generated code for {:?}", task.target_file)),
            llm_response: Some(code),
            error: None,
        })
    }

    /// Run git add and commit using std::process::Command (thread-safe)
    fn git_add_and_commit(
        &self,
        changes: &[FileChange],
        task: &Task,
        branch_name: &str,
    ) -> Result<()> {
        use std::process::Command;

        // Initialize git repo if needed
        let git_dir = self.target_dir.join(".git");
        if !git_dir.exists() {
            let output = Command::new("git")
                .args(["init"])
                .current_dir(&self.target_dir)
                .output()?;

            if !output.status.success() {
                return Err(anyhow::anyhow!("git init failed"));
            }
        }

        // Create branch
        let output = Command::new("git")
            .args(["checkout", "-b", branch_name])
            .current_dir(&self.target_dir)
            .output()?;

        // If branch already exists, use checkout -b with force or switch to it
        if !output.status.success() {
            Command::new("git")
                .args(["checkout", branch_name])
                .current_dir(&self.target_dir)
                .output()?;
        }

        // Stage all changes
        let output = Command::new("git")
            .args(["add", "-A"])
            .current_dir(&self.target_dir)
            .output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("git add failed"));
        }

        // Commit
        let msg = format!(
            "{}: {}",
            match changes.first().map(|c| &c.action) {
                Some(FileAction::Create) => "Create",
                Some(FileAction::Modify) => "Update",
                Some(FileAction::Delete) => "Delete",
                None => "Create",
            },
            task.description
        );

        let output = Command::new("git")
            .args(["commit", "-m", &msg])
            .current_dir(&self.target_dir)
            .output()?;

        if !output.status.success() {
            // Maybe nothing to commit - that's ok
            tracing::debug!("git commit returned non-zero (may be nothing to commit)");
        }

        Ok(())
    }

    /// Parse LLM response into FileChanges
    fn parse_code_response(&self, response: &str, task: &Task) -> Result<Vec<FileChange>> {
        Ok(crate::brain::parse_llm_code_response(
            response,
            &task.target_file,
        ))
    }

    /// Apply a file change to the filesystem
    async fn apply_file_change(&self, change: &FileChange) -> Result<()> {
        let full_path = self.target_dir.join(&change.path);

        // Create parent directories if needed
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        match change.action {
            FileAction::Create | FileAction::Modify => {
                std::fs::write(&full_path, &change.content)?;
                tracing::debug!(
                    file = %full_path.display(),
                    bytes = change.content.len(),
                    "File written"
                );
            }
            FileAction::Delete => {
                if full_path.exists() {
                    std::fs::remove_file(&full_path)?;
                    tracing::debug!(file = %full_path.display(), "File deleted");
                }
            }
        }

        Ok(())
    }
}

/// Spawn multiple workers concurrently
pub async fn spawn_workers(
    count: usize,
    specialists: Vec<Specialist>,
    blackboard: Arc<Blackboard>,
    brain: Arc<Brain>,
    target_dir: std::path::PathBuf,
) -> Vec<tokio::task::JoinHandle<()>> {
    let mut handles = Vec::new();

    for i in 0..count {
        let specialist = specialists
            .get(i % specialists.len())
            .cloned()
            .unwrap_or(Specialist::Generic);

        let worker = Worker::new(
            specialist.clone(),
            blackboard.clone(),
            brain.clone(),
            target_dir.clone(),
        );

        let worker_id = worker.id();
        let handle = tokio::spawn(async move {
            worker.run().await;
        });

        handles.push(handle);

        tracing::info!(
            worker_id = %worker_id,
            specialist = ?specialist,
            "Worker spawned"
        );
    }

    handles
}
