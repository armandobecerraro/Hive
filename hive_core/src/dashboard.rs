//! # Dashboard - Sistema de Monitoreo del Enjambre
//!
//! Proporciona visibilidad en tiempo real del estado del enjambre:
//! - Estado de cada obrera
//! - Progreso de tareas
//! - Métricas de calidad
//! - Historial de decisiones

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Estado de una obrera individual
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerStatus {
    pub id: Uuid,
    pub specialist_key: String,
    pub status: WorkerState,
    pub current_task: Option<String>,
    pub tasks_completed: u32,
    pub tasks_failed: u32,
    pub started_at: i64,
    pub last_activity: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkerState {
    Idle,
    Working,
    Waiting,
    Failed(String),
}

/// Snapshot del estado del enjambre
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HiveSnapshot {
    pub timestamp: i64,
    pub total_workers: usize,
    pub active_workers: usize,
    pub idle_workers: usize,
    pub failed_workers: usize,
    pub pending_tasks: usize,
    pub in_progress_tasks: usize,
    pub completed_tasks: usize,
    pub workers: Vec<WorkerStatus>,
    pub recent_events: Vec<String>,
    pub quality_metrics: QualityMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityMetrics {
    pub tests_passed: u32,
    pub tests_failed: u32,
    pub warnings_count: u32,
    pub security_issues: u32,
    pub debt_score: f32,
}

impl Default for QualityMetrics {
    fn default() -> Self {
        Self {
            tests_passed: 0,
            tests_failed: 0,
            warnings_count: 0,
            security_issues: 0,
            debt_score: 0.0,
        }
    }
}

/// Sistema de dashboard
pub struct HiveDashboard {
    workers: Arc<RwLock<HashMap<Uuid, WorkerStatus>>>,
    events: Arc<RwLock<Vec<String>>>,
    quality: Arc<RwLock<QualityMetrics>>,
    max_events: usize,
}

impl HiveDashboard {
    pub fn new() -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(RwLock::new(Vec::new())),
            quality: Arc::new(RwLock::new(QualityMetrics::default())),
            max_events: 100,
        }
    }

    /// Registrar una nueva obrera
    pub async fn register_worker(&self, worker_id: Uuid, specialist_key: String) {
        let mut workers = self.workers.write().await;
        workers.insert(
            worker_id,
            WorkerStatus {
                id: worker_id,
                specialist_key,
                status: WorkerState::Idle,
                current_task: None,
                tasks_completed: 0,
                tasks_failed: 0,
                started_at: chrono::Utc::now().timestamp(),
                last_activity: chrono::Utc::now().timestamp(),
            },
        );
    }

    /// Actualizar estado de una obrera
    pub async fn update_worker_status(
        &self,
        worker_id: Uuid,
        status: WorkerState,
        task: Option<String>,
    ) {
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(&worker_id) {
            worker.status = status;
            worker.current_task = task;
            worker.last_activity = chrono::Utc::now().timestamp();
        }
    }

    /// Registrar tarea completada
    pub async fn record_task_completed(&self, worker_id: Uuid) {
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(&worker_id) {
            worker.tasks_completed += 1;
            worker.status = WorkerState::Idle;
            worker.current_task = None;
            worker.last_activity = chrono::Utc::now().timestamp();
        }
    }

    /// Registrar tarea fallida
    pub async fn record_task_failed(&self, worker_id: Uuid, reason: String) {
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(&worker_id) {
            worker.tasks_failed += 1;
            worker.status = WorkerState::Failed(reason);
            worker.current_task = None;
            worker.last_activity = chrono::Utc::now().timestamp();
        }
    }

    /// Agregar evento al historial
    pub async fn add_event(&self, event: String) {
        let mut events = self.events.write().await;
        events.push(format!(
            "[{}] {}",
            chrono::Utc::now().format("%H:%M:%S"),
            event
        ));
        if events.len() > self.max_events {
            let to_remove = events.len() - self.max_events;
            events.drain(0..to_remove);
        }
    }

    /// Actualizar métricas de calidad
    pub async fn update_quality_metrics(&self, metrics: QualityMetrics) {
        let mut quality = self.quality.write().await;
        *quality = metrics;
    }

    /// Obtener snapshot completo del estado
    pub async fn get_snapshot(
        &self,
        pending: usize,
        in_progress: usize,
        completed: usize,
    ) -> HiveSnapshot {
        let workers = self.workers.read().await;
        let events = self.events.read().await;
        let quality = self.quality.read().await;

        let active_workers = workers
            .values()
            .filter(|w| matches!(w.status, WorkerState::Working))
            .count();
        let idle_workers = workers
            .values()
            .filter(|w| matches!(w.status, WorkerState::Idle))
            .count();
        let failed_workers = workers
            .values()
            .filter(|w| matches!(w.status, WorkerState::Failed(_)))
            .count();

        HiveSnapshot {
            timestamp: chrono::Utc::now().timestamp(),
            total_workers: workers.len(),
            active_workers,
            idle_workers,
            failed_workers,
            pending_tasks: pending,
            in_progress_tasks: in_progress,
            completed_tasks: completed,
            workers: workers.values().cloned().collect(),
            recent_events: events.clone(),
            quality_metrics: quality.clone(),
        }
    }

    /// Formatear estado como texto legible
    pub async fn format_status(
        &self,
        pending: usize,
        in_progress: usize,
        completed: usize,
    ) -> String {
        let snapshot = self.get_snapshot(pending, in_progress, completed).await;

        let mut output = String::new();
        output.push_str("╔══════════════════════════════════════════════════════════════╗\n");
        output.push_str("║                   THE HIVE - DASHBOARD                     ║\n");
        output.push_str("╚══════════════════════════════════════════════════════════════╝\n\n");

        // Resumen
        output.push_str("📊 RESUMEN\n");
        output.push_str(&format!(
            "  Obreras: {} total, {} activas, {} inactivas, {} fallidas\n",
            snapshot.total_workers,
            snapshot.active_workers,
            snapshot.idle_workers,
            snapshot.failed_workers
        ));
        output.push_str(&format!(
            "  Tareas: {} pendientes, {} en progreso, {} completadas\n\n",
            snapshot.pending_tasks, snapshot.in_progress_tasks, snapshot.completed_tasks
        ));

        // Obreras
        output.push_str("🐝 OBRERAS\n");
        for worker in &snapshot.workers {
            let status_icon = match &worker.status {
                WorkerState::Idle => "💤",
                WorkerState::Working => "🔨",
                WorkerState::Waiting => "⏳",
                WorkerState::Failed(_) => "❌",
            };
            output.push_str(&format!(
                "  {} {} ({}) - {} completadas, {} fallidas\n",
                status_icon,
                worker.specialist_key,
                &worker.id.to_string()[..8],
                worker.tasks_completed,
                worker.tasks_failed
            ));
            if let Some(task) = &worker.current_task {
                output.push_str(&format!("     └─ Trabajando en: {}\n", task));
            }
        }

        // Métricas
        output.push_str("\n📈 MÉTRICAS DE CALIDAD\n");
        output.push_str(&format!(
            "  Tests: {} pasados, {} fallidos\n",
            snapshot.quality_metrics.tests_passed, snapshot.quality_metrics.tests_failed
        ));
        output.push_str(&format!(
            "  Warnings: {}\n",
            snapshot.quality_metrics.warnings_count
        ));
        output.push_str(&format!(
            "  Issues de seguridad: {}\n",
            snapshot.quality_metrics.security_issues
        ));
        output.push_str(&format!(
            "  Score de deuda: {:.1}/100\n",
            snapshot.quality_metrics.debt_score
        ));

        // Eventos recientes
        output.push_str("\n📝 EVENTOS RECIENTES\n");
        let recent: Vec<_> = snapshot.recent_events.iter().rev().take(5).collect();
        for event in recent {
            output.push_str(&format!("  {}\n", event));
        }

        output
    }
}

impl Default for HiveDashboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dashboard_workflow() {
        let dashboard = HiveDashboard::new();

        let worker_id = Uuid::new_v4();
        dashboard
            .register_worker(worker_id, "rust".to_string())
            .await;

        dashboard
            .update_worker_status(worker_id, WorkerState::Working, Some("Tarea 1".to_string()))
            .await;
        dashboard.record_task_completed(worker_id).await;

        dashboard
            .add_event("Obrera completó tarea".to_string())
            .await;

        let snapshot = dashboard.get_snapshot(1, 1, 1).await;
        assert_eq!(snapshot.total_workers, 1);
        assert!(!snapshot.recent_events.is_empty());
    }

    #[tokio::test]
    async fn test_format_status() {
        let dashboard = HiveDashboard::new();

        let worker_id = Uuid::new_v4();
        dashboard
            .register_worker(worker_id, "python".to_string())
            .await;

        let status = dashboard.format_status(5, 2, 10).await;
        assert!(status.contains("THE HIVE"));
        assert!(status.contains("python"));
    }

    #[tokio::test]
    async fn test_all_worker_states_quality_metrics_y_eventos_recientes() {
        let d = HiveDashboard::new();
        let w_work = Uuid::new_v4();
        let w_wait = Uuid::new_v4();
        let w_fail = Uuid::new_v4();
        d.register_worker(w_work, "rust".into()).await;
        d.register_worker(w_wait, "py".into()).await;
        d.register_worker(w_fail, "go".into()).await;
        d.update_worker_status(w_work, WorkerState::Working, Some("feature-x".into()))
            .await;
        d.update_worker_status(w_wait, WorkerState::Waiting, None)
            .await;
        d.update_worker_status(w_fail, WorkerState::Failed("timeout".into()), None)
            .await;
        d.update_quality_metrics(QualityMetrics {
            tests_passed: 42,
            tests_failed: 1,
            warnings_count: 3,
            security_issues: 2,
            debt_score: 18.5,
        })
        .await;
        for i in 0..105 {
            d.add_event(format!("evt-{i}")).await;
        }
        let snap = d.get_snapshot(7, 3, 9).await;
        assert_eq!(snap.failed_workers, 1);
        assert_eq!(snap.active_workers, 1);
        assert_eq!(snap.idle_workers, 0);
        assert_eq!(snap.quality_metrics.tests_passed, 42);

        let txt = d.format_status(7, 3, 9).await;
        assert!(txt.contains("feature-x"));
        assert!(txt.contains("timeout") || txt.contains("❌"));
        assert!(txt.contains("Score de deuda"));
    }

    #[tokio::test]
    async fn test_record_task_failed_y_completado() {
        let d = HiveDashboard::new();
        let id = Uuid::new_v4();
        d.register_worker(id, "js_ts".into()).await;
        d.record_task_failed(id, "compile err".into()).await;
        let s = d.get_snapshot(0, 0, 0).await;
        assert_eq!(s.failed_workers, 1);
        d.record_task_completed(id).await;
        let s2 = d.get_snapshot(0, 0, 1).await;
        assert_eq!(s2.failed_workers, 0);
        assert_eq!(s2.completed_tasks, 1);
    }
}
