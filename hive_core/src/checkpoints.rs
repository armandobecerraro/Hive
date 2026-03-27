//! Human-in-the-loop checkpoints.
//!
//! Define puntos de aprobación humana durante el ciclo de ejecución.
//! Inspirado en CrewAI y LangGraph que permiten pausar y pedir aprobación.

use serde::{Deserialize, Serialize};

/// Tipo de checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CheckpointType {
    BeforeExecution,
    BeforeCommit,
    BeforeMerge,
    AfterError,
    BeforeScaffold,
    Custom(String),
}

/// Decisión del humano en un checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HumanDecision {
    Approve,
    Reject,
    Modify,
    Skip,
}

/// Un checkpoint configurado.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub checkpoint_type: CheckpointType,
    pub enabled: bool,
    pub message: String,
    pub auto_approve_after_secs: Option<u64>,
}

/// Configuración de checkpoints.
pub struct CheckpointManager {
    checkpoints: Vec<Checkpoint>,
}

impl CheckpointManager {
    pub fn new() -> Self {
        Self {
            checkpoints: vec![
                Checkpoint {
                    checkpoint_type: CheckpointType::BeforeExecution,
                    enabled: false,
                    message: "¿Proceder con la ejecución?".into(),
                    auto_approve_after_secs: None,
                },
                Checkpoint {
                    checkpoint_type: CheckpointType::BeforeCommit,
                    enabled: false,
                    message: "¿Commitear los cambios?".into(),
                    auto_approve_after_secs: Some(60),
                },
                Checkpoint {
                    checkpoint_type: CheckpointType::BeforeMerge,
                    enabled: true,
                    message: "¿Merge a la rama principal?".into(),
                    auto_approve_after_secs: None,
                },
                Checkpoint {
                    checkpoint_type: CheckpointType::AfterError,
                    enabled: true,
                    message: "Se encontró un error. ¿Continuar?".into(),
                    auto_approve_after_secs: Some(300),
                },
            ],
        }
    }

    /// Habilita un checkpoint.
    pub fn enable(&mut self, cp_type: &CheckpointType) {
        if let Some(cp) = self
            .checkpoints
            .iter_mut()
            .find(|c| &c.checkpoint_type == cp_type)
        {
            cp.enabled = true;
        }
    }

    /// Deshabilita un checkpoint.
    pub fn disable(&mut self, cp_type: &CheckpointType) {
        if let Some(cp) = self
            .checkpoints
            .iter_mut()
            .find(|c| &c.checkpoint_type == cp_type)
        {
            cp.enabled = false;
        }
    }

    /// Verifica si un checkpoint requiere aprobación.
    pub fn requires_approval(&self, cp_type: &CheckpointType) -> bool {
        self.checkpoints
            .iter()
            .any(|c| c.enabled && &c.checkpoint_type == cp_type)
    }

    /// Obtiene el mensaje de un checkpoint.
    pub fn get_message(&self, cp_type: &CheckpointType) -> Option<&str> {
        self.checkpoints
            .iter()
            .find(|c| &c.checkpoint_type == cp_type)
            .map(|c| c.message.as_str())
    }

    /// Lista checkpoints habilitados.
    pub fn enabled_checkpoints(&self) -> Vec<&Checkpoint> {
        self.checkpoints.iter().filter(|c| c.enabled).collect()
    }

    /// Configura desde variables de entorno.
    pub fn from_env() -> Self {
        let mut manager = Self::new();
        if let Ok(val) = std::env::var("HIVE_CHECKPOINT_BEFORE_MERGE") {
            if val == "0" || val == "false" {
                manager.disable(&CheckpointType::BeforeMerge);
            }
        }
        if let Ok(val) = std::env::var("HIVE_CHECKPOINT_ON_ERROR") {
            if val == "0" || val == "false" {
                manager.disable(&CheckpointType::AfterError);
            }
        }
        manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_checkpoints() {
        let mgr = CheckpointManager::new();
        assert!(mgr.requires_approval(&CheckpointType::BeforeMerge));
        assert!(mgr.requires_approval(&CheckpointType::AfterError));
    }

    #[test]
    fn enable_disable() {
        let mut mgr = CheckpointManager::new();
        mgr.enable(&CheckpointType::BeforeCommit);
        assert!(mgr.requires_approval(&CheckpointType::BeforeCommit));
        mgr.disable(&CheckpointType::BeforeCommit);
        assert!(!mgr.requires_approval(&CheckpointType::BeforeCommit));
    }

    #[test]
    fn enabled_checkpoints_lists() {
        let mgr = CheckpointManager::new();
        let enabled = mgr.enabled_checkpoints();
        assert!(!enabled.is_empty());
    }
}
