//! # Living Specs - Especificaciones Compartidas Vivas
//!
//! Inspirado en Augment Intent: un documento de especificación que **evoluciona**
//! durante el trabajo del enjambre. Todas las obreras leen y actualizan las specs
//! para mantenerse sincronizadas.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Decisión tomada durante el desarrollo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: Uuid,
    pub topic: String,
    pub decision: String,
    pub rationale: String,
    pub made_by: String,
    pub timestamp: i64,
    pub alternatives_considered: Vec<String>,
}

/// Criterio de aceptación
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceCriteria {
    pub id: u32,
    pub description: String,
    pub verified: bool,
    pub verified_by: Option<String>,
    pub verification_note: Option<String>,
}

/// Restricción o constraint del proyecto
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub category: String,
    pub description: String,
    pub source: String,
}

/// Especificación viva
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivingSpec {
    pub objective: String,
    pub detailed_description: String,
    pub acceptance_criteria: Vec<AcceptanceCriteria>,
    pub constraints: Vec<Constraint>,
    pub decisions: Vec<Decision>,
    pub assumptions: Vec<String>,
    pub risks: Vec<RiskItem>,
    pub updated_by: Vec<UpdateRecord>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskItem {
    pub description: String,
    pub severity: RiskSeverity,
    pub mitigation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRecord {
    pub specialist: String,
    pub worker_id: Uuid,
    pub change_description: String,
    pub timestamp: i64,
}

impl LivingSpec {
    pub fn new(objective: String) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            objective,
            detailed_description: String::new(),
            acceptance_criteria: Vec::new(),
            constraints: Vec::new(),
            decisions: Vec::new(),
            assumptions: Vec::new(),
            risks: Vec::new(),
            updated_by: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Agrega criterio de aceptación
    pub fn add_acceptance_criteria(&mut self, description: String) -> u32 {
        let id = self.acceptance_criteria.len() as u32 + 1;
        self.acceptance_criteria.push(AcceptanceCriteria {
            id,
            description,
            verified: false,
            verified_by: None,
            verification_note: None,
        });
        self.updated_at = chrono::Utc::now().timestamp();
        id
    }

    /// Verifica un criterio de aceptación
    pub fn verify_criteria(&mut self, id: u32, verified_by: &str, note: Option<String>) -> bool {
        if let Some(criteria) = self.acceptance_criteria.iter_mut().find(|c| c.id == id) {
            criteria.verified = true;
            criteria.verified_by = Some(verified_by.to_string());
            criteria.verification_note = note;
            self.updated_at = chrono::Utc::now().timestamp();
            return true;
        }
        false
    }

    /// Registra una decisión
    pub fn record_decision(
        &mut self,
        topic: String,
        decision: String,
        rationale: String,
        made_by: String,
    ) {
        self.decisions.push(Decision {
            id: Uuid::new_v4(),
            topic,
            decision,
            rationale,
            made_by,
            timestamp: chrono::Utc::now().timestamp(),
            alternatives_considered: Vec::new(),
        });
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Agrega una restricción
    pub fn add_constraint(&mut self, category: String, description: String, source: String) {
        self.constraints.push(Constraint {
            category,
            description,
            source,
        });
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Registra una actualización
    pub fn record_update(&mut self, specialist: String, worker_id: Uuid, description: String) {
        self.updated_by.push(UpdateRecord {
            specialist,
            worker_id,
            change_description: description,
            timestamp: chrono::Utc::now().timestamp(),
        });
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Agrega un riesgo
    pub fn add_risk(&mut self, description: String, severity: RiskSeverity, mitigation: String) {
        self.risks.push(RiskItem {
            description,
            severity,
            mitigation,
        });
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Verifica si todos los criterios están verificados
    pub fn all_criteria_verified(&self) -> bool {
        !self.acceptance_criteria.is_empty() && self.acceptance_criteria.iter().all(|c| c.verified)
    }

    /// Obtiene porcentaje de criterios verificados
    pub fn verification_progress(&self) -> f32 {
        if self.acceptance_criteria.is_empty() {
            return 0.0;
        }
        let verified = self
            .acceptance_criteria
            .iter()
            .filter(|c| c.verified)
            .count();
        (verified as f32 / self.acceptance_criteria.len() as f32) * 100.0
    }
}

/// Store de especificaciones vivas
pub struct LivingSpecStore {
    store_path: PathBuf,
}

impl LivingSpecStore {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            store_path: repo_root.join(".hive").join("living_spec.json"),
        }
    }

    pub fn save(&self, spec: &LivingSpec) -> anyhow::Result<()> {
        if let Some(parent) = self.store_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(spec)?;
        std::fs::write(&self.store_path, json)?;
        Ok(())
    }

    pub fn load(&self) -> anyhow::Result<Option<LivingSpec>> {
        if !self.store_path.exists() {
            return Ok(None);
        }
        let json = std::fs::read_to_string(&self.store_path)?;
        let spec: LivingSpec = serde_json::from_str(&json)?;
        Ok(Some(spec))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_spec_creation() {
        let spec = LivingSpec::new("Build API REST".into());
        assert_eq!(spec.objective, "Build API REST");
        assert!(spec.acceptance_criteria.is_empty());
    }

    #[test]
    fn test_acceptance_criteria_workflow() {
        let mut spec = LivingSpec::new("Test".into());

        let id1 = spec.add_acceptance_criteria("Endpoint responde 200".into());
        let id2 = spec.add_acceptance_criteria("Valida input".into());

        assert_eq!(spec.acceptance_criteria.len(), 2);
        assert!(!spec.all_criteria_verified());
        assert_eq!(spec.verification_progress(), 0.0);

        spec.verify_criteria(id1, "rust_worker", Some("Test manual OK".into()));
        assert_eq!(spec.verification_progress(), 50.0);

        spec.verify_criteria(id2, "test_worker", None);
        assert!(spec.all_criteria_verified());
        assert_eq!(spec.verification_progress(), 100.0);
    }

    #[test]
    fn test_decision_recording() {
        let mut spec = LivingSpec::new("Test".into());

        spec.record_decision(
            "Framework".into(),
            "Usar Actix-web".into(),
            "Mejor rendimiento".into(),
            "rust_specialist".into(),
        );

        assert_eq!(spec.decisions.len(), 1);
        assert_eq!(spec.decisions[0].topic, "Framework");
    }

    #[test]
    fn test_constraints() {
        let mut spec = LivingSpec::new("Test".into());

        spec.add_constraint(
            "Performance".into(),
            "Latencia < 100ms".into(),
            "Requerimiento".into(),
        );
        spec.add_constraint(
            "Security".into(),
            "HTTPS obligatorio".into(),
            "Policy".into(),
        );

        assert_eq!(spec.constraints.len(), 2);
    }

    #[test]
    fn test_update_tracking() {
        let mut spec = LivingSpec::new("Test".into());
        let worker_id = Uuid::new_v4();

        spec.record_update("rust".into(), worker_id, "Generé el esqueleto".into());
        spec.record_update("python".into(), Uuid::new_v4(), "Añadí tests".into());

        assert_eq!(spec.updated_by.len(), 2);
    }

    #[test]
    fn test_risks() {
        let mut spec = LivingSpec::new("Test".into());

        spec.add_risk(
            "Timeout del LLM".into(),
            RiskSeverity::Medium,
            "Retry con backoff".into(),
        );

        assert_eq!(spec.risks.len(), 1);
    }

    #[test]
    fn test_store_persistence() {
        let tmp = tempdir().unwrap();
        let store = LivingSpecStore::new(tmp.path());

        let mut spec = LivingSpec::new("API REST".into());
        spec.add_acceptance_criteria("Endpoint works".into());
        spec.record_decision(
            "DB".into(),
            "PostgreSQL".into(),
            "Mature".into(),
            "architect".into(),
        );

        store.save(&spec).unwrap();

        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.objective, "API REST");
        assert_eq!(loaded.acceptance_criteria.len(), 1);
        assert_eq!(loaded.decisions.len(), 1);
    }

    #[test]
    fn test_verification_progress_empty() {
        let spec = LivingSpec::new("Test".into());
        assert_eq!(spec.verification_progress(), 0.0);
    }

    #[test]
    fn test_all_criteria_verified_empty() {
        let spec = LivingSpec::new("Test".into());
        assert!(!spec.all_criteria_verified()); // Sin criterios = no verificado
    }
}
