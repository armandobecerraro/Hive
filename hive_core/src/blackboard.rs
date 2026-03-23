//! # Blackboard - Shared State for Multi-Agent System
//! 
//! El Blackboard es la fuente de verdad compartida:
//! - Almacena tareas pendientes
//! - Registra resultados de tareas completadas
//! - Gestiona dependencias entre tareas
//! - Workers leen y escriben sin intervención de Queen

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Tipo de especialista/agente
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Specialist {
    Rust,
    Python,
    #[serde(rename = "JavaScript")]
    JavaScript,
    #[serde(rename = "TypeScript")]
    TypeScript,
    Test,
    Docs,
    Generic,
}

impl Default for Specialist {
    fn default() -> Self {
        Specialist::Generic
    }
}

impl std::fmt::Display for Specialist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Specialist::Rust => write!(f, "Rust"),
            Specialist::Python => write!(f, "Python"),
            Specialist::JavaScript => write!(f, "JavaScript"),
            Specialist::TypeScript => write!(f, "TypeScript"),
            Specialist::Test => write!(f, "Test"),
            Specialist::Docs => write!(f, "Docs"),
            Specialist::Generic => write!(f, "Generic"),
        }
    }
}

/// Tarea pendiente en el blackboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub description: String,           // "Crear función foo() en bar.rs"
    pub target_file: PathBuf,          // "src/bar.rs"
    pub specialist_type: Specialist,    // rust, python, test, etc.
    pub priority: u8,                  // 1 = alta, 5 = baja
    pub dependencies: Vec<Uuid>,       // Tasks que deben completar primero
    pub created_by: Uuid,             // Agente que creó esta tarea
    #[serde(default)]
    pub status: TaskStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    InProgress { worker_id: Uuid },
    Completed,
    Failed { reason: String },
}

impl Default for TaskStatus {
    fn default() -> Self {
        TaskStatus::Pending
    }
}

/// Resultado de una tarea completada
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: Uuid,
    pub worker_id: Uuid,
    pub success: bool,
    pub changes: Vec<FileChange>,
    pub llm_prompt: Option<String>,
    pub llm_response: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Cambio en un archivo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: PathBuf,
    pub action: FileAction,
    pub content: String,
    pub diff: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileAction {
    Create,
    Modify,
    Delete,
}

/// Evento del sistema (para Queen observer)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkerEvent {
    #[serde(rename = "task_created")]
    TaskCreated { task: Task },
    #[serde(rename = "task_claimed")]
    TaskClaimed { task_id: Uuid, worker_id: Uuid },
    #[serde(rename = "task_completed")]
    TaskCompleted { task_id: Uuid, worker_id: Uuid, success: bool },
    #[serde(rename = "task_failed")]
    TaskFailed { task_id: Uuid, worker_id: Uuid, reason: String },
    #[serde(rename = "merge_completed")]
    MergeCompleted { branch: String, success: bool },
}

/// El Blackboard - fuente de verdad compartida
pub struct Blackboard {
    /// Tareas pendientes de procesar
    pending_tasks: RwLock<Vec<Task>>,
    /// Tareas completadas
    completed_tasks: RwLock<HashMap<Uuid, TaskResult>>,
    /// Tareas en progreso
    in_progress_tasks: RwLock<HashMap<Uuid, Task>>,
    /// Eventos para observers (Queen)
    event_tx: tokio::sync::broadcast::Sender<WorkerEvent>,
}

impl Blackboard {
    pub fn new() -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(1000);
        Self {
            pending_tasks: RwLock::new(Vec::new()),
            completed_tasks: RwLock::new(HashMap::new()),
            in_progress_tasks: RwLock::new(HashMap::new()),
            event_tx,
        }
    }
    
    /// Subscribe a observer (e.g., Queen Brain)
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<WorkerEvent> {
        self.event_tx.subscribe()
    }
    
    /// Agrega una tarea al blackboard
    pub async fn add_task(&self, task: Task) -> Result<(), anyhow::Error> {
        // Check dependencies are met
        if !task.dependencies.is_empty() {
            let completed = self.completed_tasks.read().await;
            for dep_id in &task.dependencies {
                if !completed.contains_key(dep_id) {
                    return Err(anyhow::anyhow!(
                        "Dependency {} not completed for task {}",
                        dep_id,
                        task.id
                    ));
                }
            }
        }
        
        self.pending_tasks.write().await.push(task.clone());
        
        // Notify observers
        let _ = self.event_tx.send(WorkerEvent::TaskCreated { task: task.clone() });
        
        tracing::debug!(task_id = %task.id, "Task added to blackboard");
        Ok(())
    }
    
    /// Claim una tarea (worker la toma para procesar)
    pub async fn claim_task(&self, worker_id: Uuid, specialist: &Specialist) -> Result<Option<Task>, anyhow::Error> {
        let mut pending = self.pending_tasks.write().await;
        
        // Find a suitable task
        if let Some(pos) = pending.iter().position(|t| {
            t.specialist_type == *specialist && t.status == TaskStatus::Pending
        }) {
            let task = pending.remove(pos);
            let task_id = task.id;
            
            // Mark as in progress
            drop(pending);
            let mut in_progress = self.in_progress_tasks.write().await;
            let task_with_status = Task {
                status: TaskStatus::InProgress { worker_id },
                ..task.clone()
            };
            in_progress.insert(task_id, task_with_status);
            drop(in_progress);
            
            // Notify observers
            let _ = self.event_tx.send(WorkerEvent::TaskClaimed {
                task_id,
                worker_id,
            });
            
            tracing::info!(task_id = %task_id, worker_id = %worker_id, "Task claimed");
            Ok(Some(task))
        } else {
            Ok(None) // No suitable task
        }
    }
    
    /// Complete una tarea
    pub async fn complete_task(&self, result: TaskResult) -> Result<(), anyhow::Error> {
        let task_id = result.task_id;
        let worker_id = result.worker_id;
        let success = result.success;
        
        // Remove from in_progress
        {
            let mut in_progress = self.in_progress_tasks.write().await;
            in_progress.remove(&task_id);
        }
        
        // Add to completed
        if success {
            let mut completed = self.completed_tasks.write().await;
            completed.insert(task_id, result.clone());
        }
        
        // Notify observers
        if success {
            let _ = self.event_tx.send(WorkerEvent::TaskCompleted {
                task_id,
                worker_id,
                success: true,
            });
        } else {
            let _ = self.event_tx.send(WorkerEvent::TaskFailed {
                task_id,
                worker_id,
                reason: result.error.clone().unwrap_or_default(),
            });
        }
        
        tracing::info!(task_id = %task_id, success, "Task completed");
        Ok(())
    }
    
    /// Get pending tasks count
    pub async fn pending_count(&self) -> usize {
        self.pending_tasks.read().await.len()
    }
    
    /// Get in progress tasks count
    pub async fn in_progress_count(&self) -> usize {
        self.in_progress_tasks.read().await.len()
    }
    
    /// Get completed tasks count
    pub async fn completed_count(&self) -> usize {
        self.completed_tasks.read().await.len()
    }
    
    /// Get all pending tasks (for inspection)
    pub async fn get_pending_tasks(&self) -> Vec<Task> {
        self.pending_tasks.read().await.clone()
    }
    
    /// Get context for a task (completed tasks that might be relevant)
    pub async fn get_context(&self, task: &Task) -> Vec<TaskResult> {
        let completed = self.completed_tasks.read().await;
        let mut context: Vec<TaskResult> = completed.values().cloned().collect();
        
        // Filter to only relevant tasks (same file or dependencies)
        context.retain(|r| {
            // Keep tasks that share dependencies or target files
            task.dependencies.contains(&r.task_id) ||
            r.changes.iter().any(|c| c.path == task.target_file)
        });
        
        context
    }
    
    /// Clear all completed tasks (for reset)
    pub async fn clear_completed(&self) {
        let mut completed = self.completed_tasks.write().await;
        completed.clear();
        tracing::debug!("Cleared completed tasks");
    }
    
    /// Get statistics
    pub async fn stats(&self) -> BlackboardStats {
        BlackboardStats {
            pending: self.pending_count().await,
            in_progress: self.in_progress_count().await,
            completed: self.completed_count().await,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BlackboardStats {
    pub pending: usize,
    pub in_progress: usize,
    pub completed: usize,
}

impl Default for Blackboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_add_and_claim_task() {
        let bb = Blackboard::new();
        
        let task = Task {
            id: Uuid::new_v4(),
            description: "Test task".to_string(),
            target_file: PathBuf::from("src/test.rs"),
            specialist_type: Specialist::Rust,
            priority: 1,
            dependencies: vec![],
            created_by: Uuid::nil(),
            status: TaskStatus::Pending,
        };
        
        bb.add_task(task.clone()).await.unwrap();
        
        let worker_id = Uuid::new_v4();
        let claimed = bb.claim_task(worker_id, &Specialist::Rust).await.unwrap();
        
        assert!(claimed.is_some());
        assert_eq!(bb.pending_count().await, 0);
    }
    
    #[tokio::test]
    async fn test_subscribe() {
        let bb = Blackboard::new();
        let mut rx = bb.subscribe();
        
        let task = Task {
            id: Uuid::new_v4(),
            description: "Test".to_string(),
            target_file: PathBuf::from("test.rs"),
            specialist_type: Specialist::Rust,
            priority: 1,
            dependencies: vec![],
            created_by: Uuid::nil(),
            status: TaskStatus::Pending,
        };
        
        bb.add_task(task).await.unwrap();
        
        let event = rx.recv().await.unwrap();
        match event {
            WorkerEvent::TaskCreated { .. } => {},
            _ => panic!("Expected TaskCreated"),
        }
    }
}
