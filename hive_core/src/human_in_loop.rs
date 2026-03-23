//! # Human-in-the-Loop - Control de Aprobación Humana
//!
//! Para repos de producción, ciertos cambios requieren **aprobación humana**
//! antes de mergear. Inspirado en LangGraph y BeeAI.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Tipo de acción que requiere aprobación humana
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalRequired {
    /// Cambios en archivos de seguridad
    SecurityChange { files: Vec<PathBuf> },
    /// Cambios en dependencias
    DependencyChange {
        added: Vec<String>,
        removed: Vec<String>,
    },
    /// Cambios que eliminan tests
    TestsRemoved { test_files: Vec<PathBuf> },
    /// Cambios en configuración de CI/CD
    CICDChange { files: Vec<PathBuf> },
    /// Score de calidad bajo
    LowQualityScore { score: u8, threshold: u8 },
    /// Primera contribución de una obrera
    FirstContribution { specialist: String },
    /// Cambio masivo (muchos archivos)
    MassiveChange { files_changed: u32, threshold: u32 },
}

/// Estado de una solicitud de aprobación
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalStatus {
    Pending,
    Approved {
        reviewer: String,
        timestamp: i64,
    },
    Rejected {
        reviewer: String,
        reason: String,
        timestamp: i64,
    },
    AutoApproved {
        reason: String,
    },
}

/// Solicitud de aprobación humana
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: Uuid,
    pub worker_id: Uuid,
    pub branch: String,
    pub title: String,
    pub reason: ApprovalRequired,
    pub status: ApprovalStatus,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

/// Política de aprobación
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalPolicy {
    /// Requiere aprobación para cambios de seguridad
    pub require_security_approval: bool,
    /// Requiere aprobación para eliminar tests
    pub require_test_removal_approval: bool,
    /// Umbral de score para requerir aprobación
    pub quality_score_threshold: u8,
    /// Umbral de archivos para requerir aprobación
    pub massive_change_threshold: u32,
    /// Timeout en segundos (None = sin timeout)
    pub timeout_secs: Option<u64>,
    /// Auto-aprobar después de timeout
    pub auto_approve_on_timeout: bool,
}

impl Default for ApprovalPolicy {
    fn default() -> Self {
        Self {
            require_security_approval: true,
            require_test_removal_approval: true,
            quality_score_threshold: 50,
            massive_change_threshold: 20,
            timeout_secs: Some(3600), // 1 hora
            auto_approve_on_timeout: false,
        }
    }
}

/// Gestor de aprobaciones humanas
pub struct HumanApprovalManager {
    policy: ApprovalPolicy,
    requests: Vec<ApprovalRequest>,
}

impl HumanApprovalManager {
    pub fn new(policy: ApprovalPolicy) -> Self {
        Self {
            policy,
            requests: Vec::new(),
        }
    }

    /// Evalúa si un MR requiere aprobación humana
    pub fn evaluate(
        &mut self,
        worker_id: Uuid,
        branch: &str,
        title: &str,
        files_changed: &[PathBuf],
        quality_score: u8,
    ) -> Option<ApprovalRequest> {
        let mut reasons = Vec::new();

        // Verificar seguridad
        if self.policy.require_security_approval {
            let security_files: Vec<PathBuf> = files_changed
                .iter()
                .filter(|f| {
                    let name = f.to_string_lossy().to_lowercase();
                    name.contains("auth")
                        || name.contains("security")
                        || name.contains("secret")
                        || name.contains("credential")
                        || name.contains("password")
                        || name.contains("token")
                })
                .cloned()
                .collect();

            if !security_files.is_empty() {
                reasons.push(ApprovalRequired::SecurityChange {
                    files: security_files,
                });
            }
        }

        // Verificar score de calidad
        if quality_score < self.policy.quality_score_threshold {
            reasons.push(ApprovalRequired::LowQualityScore {
                score: quality_score,
                threshold: self.policy.quality_score_threshold,
            });
        }

        // Verificar cambio masivo
        if files_changed.len() as u32 > self.policy.massive_change_threshold {
            reasons.push(ApprovalRequired::MassiveChange {
                files_changed: files_changed.len() as u32,
                threshold: self.policy.massive_change_threshold,
            });
        }

        if reasons.is_empty() {
            return None;
        }

        let primary_reason = reasons.remove(0);
        let now = chrono::Utc::now().timestamp();
        let expires_at = self.policy.timeout_secs.map(|t| now + t as i64);

        let request = ApprovalRequest {
            id: Uuid::new_v4(),
            worker_id,
            branch: branch.to_string(),
            title: title.to_string(),
            reason: primary_reason,
            status: ApprovalStatus::Pending,
            created_at: now,
            expires_at,
        };

        self.requests.push(request.clone());
        Some(request)
    }

    /// Aprueba una solicitud
    pub fn approve(&mut self, request_id: Uuid, reviewer: &str) -> bool {
        if let Some(req) = self.requests.iter_mut().find(|r| r.id == request_id) {
            req.status = ApprovalStatus::Approved {
                reviewer: reviewer.to_string(),
                timestamp: chrono::Utc::now().timestamp(),
            };
            return true;
        }
        false
    }

    /// Rechaza una solicitud
    pub fn reject(&mut self, request_id: Uuid, reviewer: &str, reason: &str) -> bool {
        if let Some(req) = self.requests.iter_mut().find(|r| r.id == request_id) {
            req.status = ApprovalStatus::Rejected {
                reviewer: reviewer.to_string(),
                reason: reason.to_string(),
                timestamp: chrono::Utc::now().timestamp(),
            };
            return true;
        }
        false
    }

    /// Verifica y procesa timeouts
    pub fn process_timeouts(&mut self) -> u32 {
        let now = chrono::Utc::now().timestamp();
        let mut processed = 0u32;

        for req in self.requests.iter_mut() {
            if req.status == ApprovalStatus::Pending {
                if let Some(expires) = req.expires_at {
                    if now > expires {
                        if self.policy.auto_approve_on_timeout {
                            req.status = ApprovalStatus::AutoApproved {
                                reason: "Timeout: auto-aprobado".into(),
                            };
                        } else {
                            req.status = ApprovalStatus::Rejected {
                                reviewer: "system".into(),
                                reason: "Timeout: solicitud expirada".into(),
                                timestamp: now,
                            };
                        }
                        processed += 1;
                    }
                }
            }
        }

        processed
    }

    /// Obtiene solicitudes pendientes
    pub fn pending_requests(&self) -> Vec<&ApprovalRequest> {
        self.requests
            .iter()
            .filter(|r| r.status == ApprovalStatus::Pending)
            .collect()
    }

    /// Lista todas las solicitudes
    pub fn list_all(&self) -> &[ApprovalRequest] {
        &self.requests
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_files() -> Vec<PathBuf> {
        vec![
            PathBuf::from("src/main.rs"),
            PathBuf::from("src/auth.rs"),
            PathBuf::from("README.md"),
        ]
    }

    #[test]
    fn test_evaluate_security_change() {
        let mut manager = HumanApprovalManager::new(ApprovalPolicy::default());
        let request = manager.evaluate(
            Uuid::new_v4(),
            "hive/worker/1",
            "Update auth",
            &sample_files(),
            80,
        );

        assert!(request.is_some());
        let req = request.unwrap();
        assert!(matches!(
            req.reason,
            ApprovalRequired::SecurityChange { .. }
        ));
    }

    #[test]
    fn test_evaluate_no_approval_needed() {
        let mut manager = HumanApprovalManager::new(ApprovalPolicy::default());
        let files = vec![PathBuf::from("src/utils.rs")];

        let request = manager.evaluate(Uuid::new_v4(), "hive/worker/1", "Fix utils", &files, 90);

        assert!(request.is_none());
    }

    #[test]
    fn test_evaluate_low_quality_score() {
        let mut manager = HumanApprovalManager::new(ApprovalPolicy::default());
        let files = vec![PathBuf::from("src/main.rs")];

        let request = manager.evaluate(
            Uuid::new_v4(),
            "hive/worker/1",
            "Quick fix",
            &files,
            30, // Bajo el threshold de 50
        );

        assert!(request.is_some());
        assert!(matches!(
            request.unwrap().reason,
            ApprovalRequired::LowQualityScore { .. }
        ));
    }

    #[test]
    fn test_evaluate_massive_change() {
        let policy = ApprovalPolicy {
            massive_change_threshold: 5,
            ..Default::default()
        };
        let mut manager = HumanApprovalManager::new(policy);

        let files: Vec<PathBuf> = (0..10)
            .map(|i| PathBuf::from(format!("src/file_{}.rs", i)))
            .collect();

        let request = manager.evaluate(Uuid::new_v4(), "hive/worker/1", "Big refactor", &files, 80);

        assert!(request.is_some());
        assert!(matches!(
            request.unwrap().reason,
            ApprovalRequired::MassiveChange { .. }
        ));
    }

    #[test]
    fn test_approve_request() {
        let mut manager = HumanApprovalManager::new(ApprovalPolicy::default());
        let files = vec![PathBuf::from("src/auth.rs")];

        let request = manager
            .evaluate(Uuid::new_v4(), "hive/worker/1", "Auth change", &files, 80)
            .unwrap();

        assert!(manager.approve(request.id, "human_reviewer"));
        assert_eq!(manager.pending_requests().len(), 0);
    }

    #[test]
    fn test_reject_request() {
        let mut manager = HumanApprovalManager::new(ApprovalPolicy::default());
        let files = vec![PathBuf::from("src/auth.rs")];

        let request = manager
            .evaluate(Uuid::new_v4(), "hive/worker/1", "Auth change", &files, 80)
            .unwrap();

        assert!(manager.reject(request.id, "reviewer", "Needs more tests"));
        assert_eq!(manager.pending_requests().len(), 0);
    }

    #[test]
    fn test_timeout_processing() {
        let policy = ApprovalPolicy {
            timeout_secs: Some(0), // Timeout inmediato
            auto_approve_on_timeout: false,
            ..Default::default()
        };
        let mut manager = HumanApprovalManager::new(policy);

        let files = vec![PathBuf::from("src/auth.rs")];
        let req = manager
            .evaluate(Uuid::new_v4(), "hive/worker/1", "Auth", &files, 80)
            .unwrap();

        // Forzar expired_at en el pasado
        if let Some(r) = manager.requests.iter_mut().find(|r| r.id == req.id) {
            r.expires_at = Some(0); // Timestamp en el pasado
        }

        let processed = manager.process_timeouts();
        assert_eq!(processed, 1);
        assert_eq!(manager.pending_requests().len(), 0);
    }

    #[test]
    fn test_auto_approve_on_timeout() {
        let policy = ApprovalPolicy {
            timeout_secs: Some(0),
            auto_approve_on_timeout: true,
            ..Default::default()
        };
        let mut manager = HumanApprovalManager::new(policy);

        let files = vec![PathBuf::from("src/auth.rs")];
        let req = manager
            .evaluate(Uuid::new_v4(), "hive/worker/1", "Auth", &files, 80)
            .unwrap();

        // Forzar expired_at en el pasado
        if let Some(r) = manager.requests.iter_mut().find(|r| r.id == req.id) {
            r.expires_at = Some(0);
        }

        manager.process_timeouts();

        let all = manager.list_all();
        assert!(matches!(all[0].status, ApprovalStatus::AutoApproved { .. }));
    }

    #[test]
    fn test_approval_request_serialization() {
        let request = ApprovalRequest {
            id: Uuid::new_v4(),
            worker_id: Uuid::new_v4(),
            branch: "main".into(),
            title: "Test".into(),
            reason: ApprovalRequired::SecurityChange { files: vec![] },
            status: ApprovalStatus::Pending,
            created_at: 1234567890,
            expires_at: Some(1234567890),
        };
        let json = serde_json::to_string(&request).unwrap();
        let deserialized: ApprovalRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.branch, "main");
    }

    #[test]
    fn test_approval_policy_default() {
        let policy = ApprovalPolicy::default();
        assert!(policy.require_security_approval);
        assert!(policy.require_test_removal_approval);
        assert_eq!(policy.quality_score_threshold, 50);
        assert_eq!(policy.massive_change_threshold, 20);
    }

    #[test]
    fn test_approval_status_equality() {
        assert_eq!(ApprovalStatus::Pending, ApprovalStatus::Pending);
        assert_ne!(
            ApprovalStatus::Pending,
            ApprovalStatus::Approved {
                reviewer: "a".into(),
                timestamp: 1
            }
        );
    }

    #[test]
    fn test_human_approval_manager_new() {
        let manager = HumanApprovalManager::new(ApprovalPolicy::default());
        assert!(manager.pending_requests().is_empty());
    }
}
