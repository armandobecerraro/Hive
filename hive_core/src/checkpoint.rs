//! # Checkpoint System - Persistencia de Estado del Workflow
//!
//! Inspirado en LangGraph: permite **guardar y restaurar** el estado completo
//! del workflow de una obrera. Si el proceso cae a mitad, se puede reanudar
//! desde el último checkpoint sin perder trabajo previo.
//!
//! ## Ejemplo
//! ```rust,ignore
//! let store = CheckpointStore::new(repo_root);
//!
//! // Guardar checkpoint
//! store.save(Checkpoint {
//!     task_id: worker_id,
//!     phase: WorkflowPhase::CodeGeneration,
//!     completed_steps: vec!["analyze".into(), "plan".into()],
//!     artifacts: HashMap::new(),
//!     metadata: HashMap::new(),
//! }).await?;
//!
//! // Restaurar
//! if let Some(cp) = store.load(worker_id).await? {
//!     // Reanudar desde la fase guardada
//! }
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Fases del workflow de una obrera
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowPhase {
    /// Análisis inicial del repo
    Analysis,
    /// Planificación de cambios
    Planning,
    /// Generación de código
    CodeGeneration,
    /// Ejecución de tests
    Testing,
    /// Commit de cambios
    Commit,
    /// Esperando revisión del Consejo
    AwaitingReview,
    /// Aplicando feedback
    ApplyingFeedback,
    /// Completado
    Completed,
    /// Fallido
    Failed,
}

impl std::fmt::Display for WorkflowPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Analysis => write!(f, "analysis"),
            Self::Planning => write!(f, "planning"),
            Self::CodeGeneration => write!(f, "code_generation"),
            Self::Testing => write!(f, "testing"),
            Self::Commit => write!(f, "commit"),
            Self::AwaitingReview => write!(f, "awaiting_review"),
            Self::ApplyingFeedback => write!(f, "applying_feedback"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// Resultado de un paso completado
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_name: String,
    pub success: bool,
    pub duration_ms: u64,
    pub output: Option<String>,
    pub error: Option<String>,
}

/// Checkpoint del estado de una obrera
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub task_id: Uuid,
    pub phase: WorkflowPhase,
    pub completed_steps: Vec<String>,
    pub step_results: Vec<StepResult>,
    pub artifacts: HashMap<String, String>,
    pub metadata: HashMap<String, String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub attempt: u32,
}

impl Checkpoint {
    pub fn new(task_id: Uuid, attempt: u32) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            task_id,
            phase: WorkflowPhase::Analysis,
            completed_steps: Vec::new(),
            step_results: Vec::new(),
            artifacts: HashMap::new(),
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
            attempt,
        }
    }

    /// Avanza a la siguiente fase
    pub fn advance_phase(&mut self, phase: WorkflowPhase) {
        self.phase = phase;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Marca un paso como completado
    pub fn complete_step(&mut self, step: &str, result: StepResult) {
        self.completed_steps.push(step.to_string());
        self.step_results.push(result);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Agrega un artefacto (archivo generado, resultado, etc.)
    pub fn add_artifact(&mut self, key: String, value: String) {
        self.artifacts.insert(key, value);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Agrega metadata
    pub fn set_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Verifica si un paso ya fue completado
    pub fn is_step_completed(&self, step: &str) -> bool {
        self.completed_steps.contains(&step.to_string())
    }

    /// Obtiene el último resultado de paso
    pub fn last_step_result(&self) -> Option<&StepResult> {
        self.step_results.last()
    }
}

/// Almacén de checkpoints
///
/// Persiste checkpoints en disco bajo `.hive/checkpoints/`.
pub struct CheckpointStore {
    store_dir: PathBuf,
}

impl CheckpointStore {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            store_dir: repo_root.join(".hive").join("checkpoints"),
        }
    }

    /// Guarda un checkpoint
    pub async fn save(&self, checkpoint: &Checkpoint) -> Result<()> {
        std::fs::create_dir_all(&self.store_dir)
            .with_context(|| format!("crear {}", self.store_dir.display()))?;

        let file_path = self.checkpoint_path(checkpoint.task_id);
        let json = serde_json::to_string_pretty(checkpoint)?;
        std::fs::write(&file_path, json)
            .with_context(|| format!("escribir checkpoint {}", file_path.display()))?;

        tracing::debug!(
            task_id = %checkpoint.task_id,
            phase = %checkpoint.phase,
            steps = checkpoint.completed_steps.len(),
            "checkpoint guardado"
        );

        Ok(())
    }

    /// Carga un checkpoint
    pub async fn load(&self, task_id: Uuid) -> Result<Option<Checkpoint>> {
        let file_path = self.checkpoint_path(task_id);
        if !file_path.exists() {
            return Ok(None);
        }

        let json = std::fs::read_to_string(&file_path)
            .with_context(|| format!("leer checkpoint {}", file_path.display()))?;
        let checkpoint: Checkpoint = serde_json::from_str(&json)?;

        Ok(Some(checkpoint))
    }

    /// Elimina un checkpoint
    pub async fn delete(&self, task_id: Uuid) -> Result<bool> {
        let file_path = self.checkpoint_path(task_id);
        if file_path.exists() {
            std::fs::remove_file(&file_path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Lista todos los checkpoints guardados
    pub async fn list(&self) -> Result<Vec<Checkpoint>> {
        if !self.store_dir.exists() {
            return Ok(Vec::new());
        }

        let mut checkpoints = Vec::new();
        for entry in std::fs::read_dir(&self.store_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(json) = std::fs::read_to_string(&path) {
                    if let Ok(cp) = serde_json::from_str::<Checkpoint>(&json) {
                        checkpoints.push(cp);
                    }
                }
            }
        }

        checkpoints.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(checkpoints)
    }

    /// Verifica si existe un checkpoint para una tarea
    pub async fn exists(&self, task_id: Uuid) -> bool {
        self.checkpoint_path(task_id).exists()
    }

    /// Limpia checkpoints antiguos (más de `max_age_secs` segundos)
    pub async fn cleanup_old(&self, max_age_secs: i64) -> Result<u32> {
        let now = chrono::Utc::now().timestamp();
        let mut cleaned = 0u32;

        for cp in self.list().await? {
            if now - cp.updated_at > max_age_secs {
                self.delete(cp.task_id).await?;
                cleaned += 1;
            }
        }

        Ok(cleaned)
    }

    fn checkpoint_path(&self, task_id: Uuid) -> PathBuf {
        self.store_dir.join(format!("{}.json", task_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_checkpoint_creation_and_modification() {
        let task_id = Uuid::new_v4();
        let mut cp = Checkpoint::new(task_id, 1);

        assert_eq!(cp.phase, WorkflowPhase::Analysis);
        assert!(cp.completed_steps.is_empty());

        cp.advance_phase(WorkflowPhase::Planning);
        assert_eq!(cp.phase, WorkflowPhase::Planning);

        cp.complete_step(
            "analyze_repo",
            StepResult {
                step_name: "analyze_repo".into(),
                success: true,
                duration_ms: 1500,
                output: Some("Found 5 files".into()),
                error: None,
            },
        );

        assert!(cp.is_step_completed("analyze_repo"));
        assert!(!cp.is_step_completed("generate_code"));
        assert!(cp.last_step_result().is_some());
    }

    #[test]
    fn test_checkpoint_artifacts_and_metadata() {
        let mut cp = Checkpoint::new(Uuid::new_v4(), 1);

        cp.add_artifact("generated_code".into(), "fn main() {}".into());
        cp.set_metadata("model_used".into(), "codellama".into());

        assert_eq!(cp.artifacts.get("generated_code").unwrap(), "fn main() {}");
        assert_eq!(cp.metadata.get("model_used").unwrap(), "codellama");
    }

    #[tokio::test]
    async fn test_store_save_and_load() {
        let tmp = tempdir().unwrap();
        let store = CheckpointStore::new(tmp.path());

        let task_id = Uuid::new_v4();
        let mut cp = Checkpoint::new(task_id, 1);
        cp.advance_phase(WorkflowPhase::CodeGeneration);
        cp.complete_step(
            "plan",
            StepResult {
                step_name: "plan".into(),
                success: true,
                duration_ms: 500,
                output: None,
                error: None,
            },
        );

        store.save(&cp).await.unwrap();

        let loaded = store.load(task_id).await.unwrap().unwrap();
        assert_eq!(loaded.phase, WorkflowPhase::CodeGeneration);
        assert_eq!(loaded.completed_steps.len(), 1);
        assert_eq!(loaded.attempt, 1);
    }

    #[tokio::test]
    async fn test_store_nonexistent_load() {
        let tmp = tempdir().unwrap();
        let store = CheckpointStore::new(tmp.path());

        let result = store.load(Uuid::new_v4()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_store_delete() {
        let tmp = tempdir().unwrap();
        let store = CheckpointStore::new(tmp.path());

        let task_id = Uuid::new_v4();
        let cp = Checkpoint::new(task_id, 1);
        store.save(&cp).await.unwrap();

        assert!(store.exists(task_id).await);
        assert!(store.delete(task_id).await.unwrap());
        assert!(!store.exists(task_id).await);
        assert!(!store.delete(task_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_store_list() {
        let tmp = tempdir().unwrap();
        let store = CheckpointStore::new(tmp.path());

        for _ in 0..3 {
            let cp = Checkpoint::new(Uuid::new_v4(), 1);
            store.save(&cp).await.unwrap();
        }

        let list = store.list().await.unwrap();
        assert_eq!(list.len(), 3);
    }

    #[tokio::test]
    async fn test_checkpoint_workflow_progression() {
        let task_id = Uuid::new_v4();
        let mut cp = Checkpoint::new(task_id, 1);

        // Simular progreso completo del workflow
        let phases = [
            (WorkflowPhase::Analysis, "analyze"),
            (WorkflowPhase::Planning, "plan"),
            (WorkflowPhase::CodeGeneration, "generate"),
            (WorkflowPhase::Testing, "test"),
            (WorkflowPhase::Commit, "commit"),
        ];

        for (phase, step) in phases {
            cp.advance_phase(phase);
            cp.complete_step(
                step,
                StepResult {
                    step_name: step.into(),
                    success: true,
                    duration_ms: 100,
                    output: None,
                    error: None,
                },
            );
        }

        assert_eq!(cp.phase, WorkflowPhase::Commit);
        assert_eq!(cp.completed_steps.len(), 5);
    }

    #[tokio::test]
    async fn test_checkpoint_retry_tracking() {
        let task_id = Uuid::new_v4();
        let mut cp = Checkpoint::new(task_id, 1);

        // Primer intento falla
        cp.advance_phase(WorkflowPhase::CodeGeneration);
        cp.complete_step(
            "generate",
            StepResult {
                step_name: "generate".into(),
                success: false,
                duration_ms: 100,
                output: None,
                error: Some("LLM timeout".into()),
            },
        );

        // Segundo intento
        let mut cp2 = Checkpoint::new(task_id, 2);
        cp2.advance_phase(WorkflowPhase::CodeGeneration);
        cp2.complete_step(
            "generate",
            StepResult {
                step_name: "generate".into(),
                success: true,
                duration_ms: 200,
                output: Some("fn main() {}".into()),
                error: None,
            },
        );

        assert_eq!(cp.attempt, 1);
        assert_eq!(cp2.attempt, 2);
        assert!(cp2.last_step_result().unwrap().success);
    }

    #[test]
    fn workflow_phase_display_cubre_todas_las_variantes() {
        use std::fmt::Write;
        let mut s = String::new();
        for p in [
            WorkflowPhase::Analysis,
            WorkflowPhase::Planning,
            WorkflowPhase::CodeGeneration,
            WorkflowPhase::Testing,
            WorkflowPhase::Commit,
            WorkflowPhase::AwaitingReview,
            WorkflowPhase::ApplyingFeedback,
            WorkflowPhase::Completed,
            WorkflowPhase::Failed,
        ] {
            write!(&mut s, "{p} ").unwrap();
        }
        assert!(s.contains("testing"));
        assert!(s.contains("awaiting_review"));
        assert!(s.contains("failed"));
    }

    #[tokio::test]
    async fn test_cleanup_old_borra_antiguos() {
        let tmp = tempdir().unwrap();
        let store = CheckpointStore::new(tmp.path());
        let task_id = Uuid::new_v4();
        let mut cp = Checkpoint::new(task_id, 1);
        cp.updated_at = chrono::Utc::now().timestamp() - 100_000;
        store.save(&cp).await.unwrap();
        let n = store.cleanup_old(3600).await.unwrap();
        assert!(n >= 1);
        assert!(!store.exists(task_id).await);
    }

    #[tokio::test]
    async fn test_list_ignora_json_invalido_y_no_json() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join(".hive").join("checkpoints");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bad.json"), "not json").unwrap();
        std::fs::write(dir.join("readme.txt"), "{}").unwrap();
        let store = CheckpointStore::new(tmp.path());
        let list = store.list().await.unwrap();
        assert!(list.is_empty());
    }
}
