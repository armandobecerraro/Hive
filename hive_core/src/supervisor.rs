//! # Worker Supervisor - Fault Tolerance
//!
//! Inspirado en OpenAI Symphony (BEAM/OTP): árbol de supervisión que **reinicia
//! obreras fallidas** automáticamente con backoff exponencial.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Política de reinicio
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartPolicy {
    pub max_restarts: u32,
    pub restart_delay_ms: u64,
    pub backoff_multiplier: f32,
    pub max_delay_ms: u64,
    pub escalate_after: u32,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_restarts: 3,
            restart_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30_000,
            escalate_after: 2,
        }
    }
}

/// Estado de una obrera supervisada
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerHealth {
    Healthy,
    Degraded { reason: String },
    Failed { reason: String, restart_count: u32 },
    Terminated { reason: String },
}

/// Registro de un intento de reinicio
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartRecord {
    pub attempt: u32,
    pub timestamp: i64,
    pub delay_ms: u64,
    pub success: bool,
    pub error: Option<String>,
}

/// Entrada de supervisión para una obrera
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisedWorker {
    pub worker_id: Uuid,
    pub specialist_key: String,
    pub health: WorkerHealth,
    pub restart_history: Vec<RestartRecord>,
    pub created_at: i64,
    pub last_restart_at: Option<i64>,
    pub consecutive_failures: u32,
}

/// Evento de supervisión
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SupervisorEvent {
    WorkerRegistered { worker_id: Uuid },
    WorkerHealthy { worker_id: Uuid },
    WorkerDegraded { worker_id: Uuid, reason: String },
    WorkerFailed { worker_id: Uuid, reason: String },
    WorkerRestarted { worker_id: Uuid, attempt: u32 },
    WorkerTerminated { worker_id: Uuid, reason: String },
    EscalationRequired { worker_id: Uuid, reason: String },
}

/// Supervisor de obreras
pub struct WorkerSupervisor {
    policy: RestartPolicy,
    workers: HashMap<Uuid, SupervisedWorker>,
    events: Vec<SupervisorEvent>,
}

impl WorkerSupervisor {
    pub fn new(policy: RestartPolicy) -> Self {
        Self {
            policy,
            workers: HashMap::new(),
            events: Vec::new(),
        }
    }

    /// Registra una obrera para supervisión
    pub fn register(&mut self, worker_id: Uuid, specialist_key: String) {
        self.workers.insert(
            worker_id,
            SupervisedWorker {
                worker_id,
                specialist_key,
                health: WorkerHealth::Healthy,
                restart_history: Vec::new(),
                created_at: chrono::Utc::now().timestamp(),
                last_restart_at: None,
                consecutive_failures: 0,
            },
        );
        self.events
            .push(SupervisorEvent::WorkerRegistered { worker_id });
    }

    /// Reporta que una obrera falló
    pub fn report_failure(&mut self, worker_id: Uuid, reason: String) -> SupervisorAction {
        let worker = match self.workers.get_mut(&worker_id) {
            Some(w) => w,
            None => return SupervisorAction::Ignore,
        };

        worker.consecutive_failures += 1;
        worker.health = WorkerHealth::Failed {
            reason: reason.clone(),
            restart_count: worker.restart_history.len() as u32,
        };

        self.events.push(SupervisorEvent::WorkerFailed {
            worker_id,
            reason: reason.clone(),
        });

        // Verificar si debemos reiniciar
        if worker.restart_history.len() as u32 >= self.policy.max_restarts {
            worker.health = WorkerHealth::Terminated {
                reason: format!("Max restarts alcanzado: {reason}"),
            };
            self.events.push(SupervisorEvent::WorkerTerminated {
                worker_id,
                reason: "max_restarts".into(),
            });
            return SupervisorAction::Terminate {
                reason: "Max restarts",
            };
        }

        // Calcular delay con backoff
        let attempt = worker.restart_history.len() as u32;
        let base_delay = self.policy.restart_delay_ms;
        let delay = (base_delay as f32 * self.policy.backoff_multiplier.powi(attempt as i32))
            .min(self.policy.max_delay_ms as f32) as u64;

        // Verificar si debemos escalar
        if attempt >= self.policy.escalate_after {
            self.events.push(SupervisorEvent::EscalationRequired {
                worker_id,
                reason: format!("{attempt} reinicios fallidos"),
            });
        }

        self.events.push(SupervisorEvent::WorkerRestarted {
            worker_id,
            attempt: attempt + 1,
        });

        SupervisorAction::Restart {
            delay_ms: delay,
            attempt: attempt + 1,
        }
    }

    /// Reporta que una obrera está degradada
    pub fn report_degraded(&mut self, worker_id: Uuid, reason: String) {
        if let Some(worker) = self.workers.get_mut(&worker_id) {
            worker.health = WorkerHealth::Degraded {
                reason: reason.clone(),
            };
            self.events
                .push(SupervisorEvent::WorkerDegraded { worker_id, reason });
        }
    }

    /// Reporta que una obrera se recuperó
    pub fn report_healthy(&mut self, worker_id: Uuid) {
        if let Some(worker) = self.workers.get_mut(&worker_id) {
            worker.health = WorkerHealth::Healthy;
            worker.consecutive_failures = 0;
            self.events
                .push(SupervisorEvent::WorkerHealthy { worker_id });
        }
    }

    /// Registra un reinicio exitoso
    pub fn record_restart_success(&mut self, worker_id: Uuid, attempt: u32, delay_ms: u64) {
        if let Some(worker) = self.workers.get_mut(&worker_id) {
            worker.restart_history.push(RestartRecord {
                attempt,
                timestamp: chrono::Utc::now().timestamp(),
                delay_ms,
                success: true,
                error: None,
            });
            worker.last_restart_at = Some(chrono::Utc::now().timestamp());
            worker.health = WorkerHealth::Healthy;
            worker.consecutive_failures = 0;
        }
    }

    /// Obtiene el estado de una obrera
    pub fn get_health(&self, worker_id: Uuid) -> Option<&WorkerHealth> {
        self.workers.get(&worker_id).map(|w| &w.health)
    }

    /// Lista todas las obreras supervisadas
    pub fn list_workers(&self) -> Vec<&SupervisedWorker> {
        self.workers.values().collect()
    }

    /// Lista obreras fallidas
    pub fn list_failed(&self) -> Vec<&SupervisedWorker> {
        self.workers
            .values()
            .filter(|w| {
                matches!(
                    w.health,
                    WorkerHealth::Failed { .. } | WorkerHealth::Terminated { .. }
                )
            })
            .collect()
    }

    /// Obtiene eventos de supervisión
    pub fn get_events(&self) -> &[SupervisorEvent] {
        &self.events
    }

    /// Estadísticas
    pub fn stats(&self) -> SupervisorStats {
        let total = self.workers.len();
        let healthy = self
            .workers
            .values()
            .filter(|w| w.health == WorkerHealth::Healthy)
            .count();
        let failed = self
            .workers
            .values()
            .filter(|w| matches!(w.health, WorkerHealth::Failed { .. }))
            .count();
        let terminated = self
            .workers
            .values()
            .filter(|w| matches!(w.health, WorkerHealth::Terminated { .. }))
            .count();

        SupervisorStats {
            total_workers: total,
            healthy,
            failed,
            terminated,
            total_restarts: self.workers.values().map(|w| w.restart_history.len()).sum(),
        }
    }
}

/// Acción que el supervisor decide tomar
#[derive(Debug, Clone)]
pub enum SupervisorAction {
    /// No hacer nada
    Ignore,
    /// Reiniciar la obrera
    Restart { delay_ms: u64, attempt: u32 },
    /// Terminar permanentemente
    Terminate { reason: &'static str },
}

#[derive(Debug, Clone, Serialize)]
pub struct SupervisorStats {
    pub total_workers: usize,
    pub healthy: usize,
    pub failed: usize,
    pub terminated: usize,
    pub total_restarts: usize,
}

impl Default for WorkerSupervisor {
    fn default() -> Self {
        Self::new(RestartPolicy::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_health() {
        let mut supervisor = WorkerSupervisor::default();
        let worker_id = Uuid::new_v4();

        supervisor.register(worker_id, "rust".into());
        assert_eq!(
            *supervisor.get_health(worker_id).unwrap(),
            WorkerHealth::Healthy
        );
    }

    #[test]
    fn test_failure_and_restart() {
        let mut supervisor = WorkerSupervisor::default();
        let worker_id = Uuid::new_v4();

        supervisor.register(worker_id, "rust".into());
        let action = supervisor.report_failure(worker_id, "OOM".into());

        match action {
            SupervisorAction::Restart { attempt, .. } => assert_eq!(attempt, 1),
            _ => panic!("Expected restart"),
        }
    }

    #[test]
    fn test_max_restarts_terminates() {
        let policy = RestartPolicy {
            max_restarts: 2,
            ..Default::default()
        };
        let mut supervisor = WorkerSupervisor::new(policy);
        let worker_id = Uuid::new_v4();

        supervisor.register(worker_id, "rust".into());

        // Primer fallo
        let action = supervisor.report_failure(worker_id, "error1".into());
        assert!(matches!(action, SupervisorAction::Restart { .. }));
        supervisor.record_restart_success(worker_id, 1, 1000);

        // Segundo fallo
        let action = supervisor.report_failure(worker_id, "error2".into());
        assert!(matches!(action, SupervisorAction::Restart { .. }));
        supervisor.record_restart_success(worker_id, 2, 2000);

        // Tercer fallo -> terminar
        let action = supervisor.report_failure(worker_id, "error3".into());
        assert!(matches!(action, SupervisorAction::Terminate { .. }));
    }

    #[test]
    fn test_backoff_calculation() {
        let policy = RestartPolicy {
            max_restarts: 5,
            restart_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 10_000,
            ..Default::default()
        };
        let mut supervisor = WorkerSupervisor::new(policy);
        let worker_id = Uuid::new_v4();

        supervisor.register(worker_id, "rust".into());

        // Primer restart: 1000ms
        let action = supervisor.report_failure(worker_id, "err".into());
        if let SupervisorAction::Restart { delay_ms, .. } = action {
            assert_eq!(delay_ms, 1000);
        }
        supervisor.record_restart_success(worker_id, 1, 1000);

        // Segundo restart: 2000ms
        let action = supervisor.report_failure(worker_id, "err".into());
        if let SupervisorAction::Restart { delay_ms, .. } = action {
            assert_eq!(delay_ms, 2000);
        }
        supervisor.record_restart_success(worker_id, 2, 2000);

        // Tercer restart: 4000ms
        let action = supervisor.report_failure(worker_id, "err".into());
        if let SupervisorAction::Restart { delay_ms, .. } = action {
            assert_eq!(delay_ms, 4000);
        }
    }

    #[test]
    fn test_healthy_resets_failures() {
        let mut supervisor = WorkerSupervisor::default();
        let worker_id = Uuid::new_v4();

        supervisor.register(worker_id, "rust".into());
        supervisor.report_failure(worker_id, "err".into());
        supervisor.record_restart_success(worker_id, 1, 1000);

        supervisor.report_healthy(worker_id);

        let worker = &supervisor.workers[&worker_id];
        assert_eq!(worker.consecutive_failures, 0);
        assert_eq!(worker.health, WorkerHealth::Healthy);
    }

    #[test]
    fn test_escalation_event() {
        let policy = RestartPolicy {
            max_restarts: 5,
            escalate_after: 2,
            ..Default::default()
        };
        let mut supervisor = WorkerSupervisor::new(policy);
        let worker_id = Uuid::new_v4();

        supervisor.register(worker_id, "rust".into());

        // Primer y segundo fallo
        supervisor.report_failure(worker_id, "err".into());
        supervisor.record_restart_success(worker_id, 1, 1000);
        supervisor.report_failure(worker_id, "err".into());
        supervisor.record_restart_success(worker_id, 2, 2000);

        // Tercer fallo -> escalación
        supervisor.report_failure(worker_id, "err".into());

        let has_escalation = supervisor
            .events
            .iter()
            .any(|e| matches!(e, SupervisorEvent::EscalationRequired { .. }));
        assert!(has_escalation);
    }

    #[test]
    fn test_stats() {
        let mut supervisor = WorkerSupervisor::default();

        supervisor.register(Uuid::new_v4(), "rust".into());
        supervisor.register(Uuid::new_v4(), "python".into());
        supervisor.register(Uuid::new_v4(), "js".into());

        let id = Uuid::new_v4();
        supervisor.register(id, "test".into());
        supervisor.report_failure(id, "err".into());

        let stats = supervisor.stats();
        assert_eq!(stats.total_workers, 4);
        assert_eq!(stats.healthy, 3);
        assert_eq!(stats.failed, 1);
    }

    #[test]
    fn test_list_failed() {
        let mut supervisor = WorkerSupervisor::default();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        supervisor.register(id1, "rust".into());
        supervisor.register(id2, "python".into());

        supervisor.report_failure(id1, "err".into());

        let failed = supervisor.list_failed();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].worker_id, id1);
    }
}
