//! # Blackboard - Shared State for Multi-Agent System
//!
//! El Blackboard es la fuente de verdad compartida:
//! - Almacena tareas pendientes
//! - Registra resultados de tareas completadas
//! - Gestiona dependencias entre tareas
//! - Workers leen y escriben sin intervención de Queen
//! - Sistema de mensajes entre obreras (pub/sub)

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
    pub description: String,         // "Crear función foo() en bar.rs"
    pub target_file: PathBuf,        // "src/bar.rs"
    pub specialist_type: Specialist, // rust, python, test, etc.
    pub priority: u8,                // 1 = alta, 5 = baja
    pub dependencies: Vec<Uuid>,     // Tasks que deben completar primero
    pub created_by: Uuid,            // Agente que creó esta tarea
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
    TaskCompleted {
        task_id: Uuid,
        worker_id: Uuid,
        success: bool,
    },
    #[serde(rename = "task_failed")]
    TaskFailed {
        task_id: Uuid,
        worker_id: Uuid,
        reason: String,
    },
    #[serde(rename = "merge_completed")]
    MergeCompleted { branch: String, success: bool },
    /// Mensaje directo entre obreras
    #[serde(rename = "inter_worker_message")]
    InterWorkerMessage {
        from: Uuid,
        to: Option<Uuid>, // None = broadcast
        message: String,
    },
}

/// Mensaje inter-obrera con contexto relevante
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerMessage {
    pub from_worker: Uuid,
    pub from_specialist: Specialist,
    pub to_worker: Option<Uuid>, // None = broadcast
    pub message_type: MessageType,
    pub content: String,
    pub related_files: Vec<PathBuf>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    /// Información sobre cambios realizados
    ChangesCompleted,
    /// Advertencia sobre conflicto potencial
    ConflictWarning,
    /// Solicitud de contexto sobre un archivo
    ContextRequest,
    /// Respuesta con contexto
    ContextResponse,
    /// Sugerencia de mejora
    Suggestion,
}

/// Historial de cambios por especialista (para que las obreras sepan qué se hizo)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecialistHistory {
    pub specialist_key: String,
    pub changes: Vec<ChangeRecord>,
    pub last_updated: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeRecord {
    pub file: PathBuf,
    pub action: FileAction,
    pub description: String,
    pub worker_id: Uuid,
    pub timestamp: i64,
}

/// Contexto compartido que las obreras pueden consultar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedContext {
    pub recent_changes: Vec<ChangeRecord>,
    pub active_specialists: Vec<String>,
    pub warnings: Vec<String>,
    pub suggestions: Vec<String>,
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
    /// Mensajes entre obreras
    messages: RwLock<Vec<WorkerMessage>>,
    /// Historial por especialista
    specialist_history: RwLock<HashMap<String, SpecialistHistory>>,
    /// Canal para mensajes inter-obrera
    message_tx: tokio::sync::broadcast::Sender<WorkerMessage>,
}

impl Blackboard {
    pub fn new() -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(1000);
        let (message_tx, _) = tokio::sync::broadcast::channel(500);
        Self {
            pending_tasks: RwLock::new(Vec::new()),
            completed_tasks: RwLock::new(HashMap::new()),
            in_progress_tasks: RwLock::new(HashMap::new()),
            event_tx,
            messages: RwLock::new(Vec::new()),
            specialist_history: RwLock::new(HashMap::new()),
            message_tx,
        }
    }

    /// Subscribe a observer (e.g., Queen Brain)
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<WorkerEvent> {
        self.event_tx.subscribe()
    }

    /// Suscribirse a mensajes entre obreras
    pub fn subscribe_messages(&self) -> tokio::sync::broadcast::Receiver<WorkerMessage> {
        self.message_tx.subscribe()
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
        let _ = self
            .event_tx
            .send(WorkerEvent::TaskCreated { task: task.clone() });

        tracing::debug!(task_id = %task.id, "Task added to blackboard");
        Ok(())
    }

    /// Claim una tarea (worker la toma para procesar)
    pub async fn claim_task(
        &self,
        worker_id: Uuid,
        specialist: &Specialist,
    ) -> Result<Option<Task>, anyhow::Error> {
        let mut pending = self.pending_tasks.write().await;

        // Find a suitable task
        if let Some(pos) = pending
            .iter()
            .position(|t| t.specialist_type == *specialist && t.status == TaskStatus::Pending)
        {
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
            let _ = self
                .event_tx
                .send(WorkerEvent::TaskClaimed { task_id, worker_id });

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

    /// Enviar mensaje a otra obrera o broadcast
    pub async fn send_message(&self, msg: WorkerMessage) -> Result<(), anyhow::Error> {
        // Almacenar mensaje
        self.messages.write().await.push(msg.clone());

        // Broadcast a suscriptores
        let _ = self.message_tx.send(msg);

        Ok(())
    }

    /// Obtener mensajes recientes (últimos N)
    pub async fn get_recent_messages(&self, limit: usize) -> Vec<WorkerMessage> {
        let messages = self.messages.read().await;
        let start = if messages.len() > limit {
            messages.len() - limit
        } else {
            0
        };
        messages[start..].to_vec()
    }

    /// Registrar cambios realizados por un especialista
    pub async fn record_specialist_change(
        &self,
        specialist_key: &str,
        file: PathBuf,
        action: FileAction,
        description: String,
        worker_id: Uuid,
    ) -> Result<(), anyhow::Error> {
        let mut history = self.specialist_history.write().await;
        let entry = history
            .entry(specialist_key.to_string())
            .or_insert_with(|| SpecialistHistory {
                specialist_key: specialist_key.to_string(),
                changes: Vec::new(),
                last_updated: chrono::Utc::now().timestamp(),
            });

        entry.changes.push(ChangeRecord {
            file,
            action,
            description,
            worker_id,
            timestamp: chrono::Utc::now().timestamp(),
        });
        entry.last_updated = chrono::Utc::now().timestamp();

        // Mantener solo los últimos 100 cambios
        if entry.changes.len() > 100 {
            entry.changes.drain(0..entry.changes.len() - 100);
        }

        Ok(())
    }

    /// Obtener contexto compartido para una obrera
    pub async fn get_shared_context(&self, _specialist: &Specialist) -> SharedContext {
        let history = self.specialist_history.read().await;
        let messages = self.messages.read().await;
        let in_progress = self.in_progress_tasks.read().await;

        // Recopilar cambios recientes de todos los especialistas
        let mut recent_changes: Vec<ChangeRecord> = Vec::new();
        for h in history.values() {
            recent_changes.extend(h.changes.iter().cloned());
        }
        recent_changes.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        recent_changes.truncate(50); // Solo los 50 más recientes

        // Especialistas activos
        let active_specialists: Vec<String> = in_progress
            .values()
            .map(|t| format!("{:?}", t.specialist_type))
            .collect();

        // Advertencias de mensajes recientes
        let warnings: Vec<String> = messages
            .iter()
            .filter(|m| matches!(m.message_type, MessageType::ConflictWarning))
            .map(|m| m.content.clone())
            .collect();

        // Sugerencias
        let suggestions: Vec<String> = messages
            .iter()
            .filter(|m| matches!(m.message_type, MessageType::Suggestion))
            .map(|m| m.content.clone())
            .collect();

        SharedContext {
            recent_changes,
            active_specialists,
            warnings,
            suggestions,
        }
    }

    /// Obtener historial de un especialista específico
    pub async fn get_specialist_history(&self, specialist_key: &str) -> Option<SpecialistHistory> {
        self.specialist_history
            .read()
            .await
            .get(specialist_key)
            .cloned()
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
            task.dependencies.contains(&r.task_id)
                || r.changes.iter().any(|c| c.path == task.target_file)
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
            messages: self.messages.read().await.len(),
            specialists_tracked: self.specialist_history.read().await.len(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BlackboardStats {
    pub pending: usize,
    pub in_progress: usize,
    pub completed: usize,
    pub messages: usize,
    pub specialists_tracked: usize,
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
            WorkerEvent::TaskCreated { .. } => {}
            _ => panic!("Expected TaskCreated"),
        }
    }

    #[tokio::test]
    async fn test_inter_worker_messaging() {
        let bb = Blackboard::new();
        let mut rx = bb.subscribe_messages();

        let msg = WorkerMessage {
            from_worker: Uuid::new_v4(),
            from_specialist: Specialist::Rust,
            to_worker: None,
            message_type: MessageType::ChangesCompleted,
            content: "Completé los cambios en main.rs".to_string(),
            related_files: vec![PathBuf::from("src/main.rs")],
            timestamp: chrono::Utc::now().timestamp(),
        };

        bb.send_message(msg.clone()).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.content, "Completé los cambios en main.rs");
    }

    #[tokio::test]
    async fn test_specialist_history() {
        let bb = Blackboard::new();

        bb.record_specialist_change(
            "rust",
            PathBuf::from("src/main.rs"),
            FileAction::Modify,
            "Añadida función foo()".to_string(),
            Uuid::new_v4(),
        )
        .await
        .unwrap();

        let history = bb.get_specialist_history("rust").await.unwrap();
        assert_eq!(history.changes.len(), 1);
        assert_eq!(history.changes[0].description, "Añadida función foo()");
    }

    #[tokio::test]
    async fn test_shared_context() {
        let bb = Blackboard::new();

        bb.record_specialist_change(
            "rust",
            PathBuf::from("src/lib.rs"),
            FileAction::Modify,
            "Refactor".to_string(),
            Uuid::new_v4(),
        )
        .await
        .unwrap();

        let ctx = bb.get_shared_context(&Specialist::Python).await;
        assert!(!ctx.recent_changes.is_empty());
    }
}
