//! # Role System - Especialización Granular de Obreras
//!
//! Inspirado en MetaGPT y CrewAI: define **roles especializados** más allá del
//! simple "obrera Rust". Cada rol tiene responsabilidades, herramientas y
//! criterios de éxito específicos.

use serde::{Deserialize, Serialize};

/// Roles especializados en el enjambre
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HiveRole {
    /// Define QUÉ hacer (prioridades, alcance)
    ProductManager,
    /// Diseña CÓMO hacerlo (arquitectura, patrones)
    Architect,
    /// Implementa el código
    Engineer,
    /// Escribe y ejecuta tests
    QualityAssurance,
    /// Genera documentación
    TechWriter,
    /// Auditoría de seguridad
    SecurityAuditor,
    /// Optimización de rendimiento
    PerformanceEngineer,
    /// Revisión de código
    CodeReviewer,
    /// Agente genérico
    Generalist,
}

impl std::fmt::Display for HiveRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProductManager => write!(f, "Product Manager"),
            Self::Architect => write!(f, "Architect"),
            Self::Engineer => write!(f, "Engineer"),
            Self::QualityAssurance => write!(f, "QA"),
            Self::TechWriter => write!(f, "Tech Writer"),
            Self::SecurityAuditor => write!(f, "Security Auditor"),
            Self::PerformanceEngineer => write!(f, "Performance Engineer"),
            Self::CodeReviewer => write!(f, "Code Reviewer"),
            Self::Generalist => write!(f, "Generalist"),
        }
    }
}

/// Capacidades de un rol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleCapabilities {
    pub can_write_code: bool,
    pub can_review_code: bool,
    pub can_write_tests: bool,
    pub can_write_docs: bool,
    pub can_design_architecture: bool,
    pub can_audit_security: bool,
    pub can_optimize_performance: bool,
    pub can_make_decisions: bool,
}

/// Configuración completa de un rol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleConfig {
    pub role: HiveRole,
    pub name: String,
    pub description: String,
    pub capabilities: RoleCapabilities,
    pub tools: Vec<String>,
    pub priority: u8,
}

impl HiveRole {
    /// Obtiene la configuración completa del rol
    pub fn config(&self) -> RoleConfig {
        match self {
            Self::ProductManager => RoleConfig {
                role: *self,
                name: "Product Manager".into(),
                description: "Define objetivos, prioridades y criterios de aceptación".into(),
                capabilities: RoleCapabilities {
                    can_write_code: false,
                    can_review_code: false,
                    can_write_tests: false,
                    can_write_docs: true,
                    can_design_architecture: false,
                    can_audit_security: false,
                    can_optimize_performance: false,
                    can_make_decisions: true,
                },
                tools: vec!["analysis".into(), "prioritization".into()],
                priority: 1,
            },
            Self::Architect => RoleConfig {
                role: *self,
                name: "Architect".into(),
                description: "Diseña la estructura y patrones del código".into(),
                capabilities: RoleCapabilities {
                    can_write_code: false,
                    can_review_code: true,
                    can_write_tests: false,
                    can_write_docs: true,
                    can_design_architecture: true,
                    can_audit_security: false,
                    can_optimize_performance: true,
                    can_make_decisions: true,
                },
                tools: vec!["analysis".into(), "diagram".into()],
                priority: 2,
            },
            Self::Engineer => RoleConfig {
                role: *self,
                name: "Engineer".into(),
                description: "Implementa código fuente".into(),
                capabilities: RoleCapabilities {
                    can_write_code: true,
                    can_review_code: false,
                    can_write_tests: false,
                    can_write_docs: false,
                    can_design_architecture: false,
                    can_audit_security: false,
                    can_optimize_performance: false,
                    can_make_decisions: false,
                },
                tools: vec!["code_generation".into(), "refactoring".into()],
                priority: 3,
            },
            Self::QualityAssurance => RoleConfig {
                role: *self,
                name: "Quality Assurance".into(),
                description: "Escribe tests y valida calidad".into(),
                capabilities: RoleCapabilities {
                    can_write_code: true,
                    can_review_code: false,
                    can_write_tests: true,
                    can_write_docs: false,
                    can_design_architecture: false,
                    can_audit_security: false,
                    can_optimize_performance: false,
                    can_make_decisions: false,
                },
                tools: vec!["test_generation".into(), "cargo_test".into()],
                priority: 4,
            },
            Self::TechWriter => RoleConfig {
                role: *self,
                name: "Tech Writer".into(),
                description: "Genera documentación técnica".into(),
                capabilities: RoleCapabilities {
                    can_write_code: false,
                    can_review_code: false,
                    can_write_tests: false,
                    can_write_docs: true,
                    can_design_architecture: false,
                    can_audit_security: false,
                    can_optimize_performance: false,
                    can_make_decisions: false,
                },
                tools: vec!["markdown".into(), "docs_generation".into()],
                priority: 5,
            },
            Self::SecurityAuditor => RoleConfig {
                role: *self,
                name: "Security Auditor".into(),
                description: "Audita seguridad del código".into(),
                capabilities: RoleCapabilities {
                    can_write_code: false,
                    can_review_code: true,
                    can_write_tests: false,
                    can_write_docs: false,
                    can_design_architecture: false,
                    can_audit_security: true,
                    can_optimize_performance: false,
                    can_make_decisions: false,
                },
                tools: vec!["security_scan".into(), "dependency_audit".into()],
                priority: 2,
            },
            Self::PerformanceEngineer => RoleConfig {
                role: *self,
                name: "Performance Engineer".into(),
                description: "Optimiza rendimiento del código".into(),
                capabilities: RoleCapabilities {
                    can_write_code: true,
                    can_review_code: true,
                    can_write_tests: false,
                    can_write_docs: false,
                    can_design_architecture: false,
                    can_audit_security: false,
                    can_optimize_performance: true,
                    can_make_decisions: false,
                },
                tools: vec!["profiling".into(), "benchmarking".into()],
                priority: 3,
            },
            Self::CodeReviewer => RoleConfig {
                role: *self,
                name: "Code Reviewer".into(),
                description: "Revisa calidad y estilo del código".into(),
                capabilities: RoleCapabilities {
                    can_write_code: false,
                    can_review_code: true,
                    can_write_tests: false,
                    can_write_docs: false,
                    can_design_architecture: false,
                    can_audit_security: false,
                    can_optimize_performance: false,
                    can_make_decisions: false,
                },
                tools: vec!["clippy".into(), "code_review".into()],
                priority: 4,
            },
            Self::Generalist => RoleConfig {
                role: *self,
                name: "Generalist".into(),
                description: "Agente genérico multifunción".into(),
                capabilities: RoleCapabilities {
                    can_write_code: true,
                    can_review_code: true,
                    can_write_tests: true,
                    can_write_docs: true,
                    can_design_architecture: false,
                    can_audit_security: false,
                    can_optimize_performance: false,
                    can_make_decisions: false,
                },
                tools: vec!["general".into()],
                priority: 5,
            },
        }
    }

    /// Obtiene todos los roles disponibles
    pub fn all() -> Vec<HiveRole> {
        vec![
            Self::ProductManager,
            Self::Architect,
            Self::Engineer,
            Self::QualityAssurance,
            Self::TechWriter,
            Self::SecurityAuditor,
            Self::PerformanceEngineer,
            Self::CodeReviewer,
            Self::Generalist,
        ]
    }

    /// Determina el rol más adecuado para una tarea
    pub fn for_task(task_description: &str) -> HiveRole {
        let desc = task_description.to_lowercase();

        if desc.contains("security") || desc.contains("vulnerability") || desc.contains("auth") {
            Self::SecurityAuditor
        } else if desc.contains("test") || desc.contains("coverage") {
            Self::QualityAssurance
        } else if desc.contains("doc") || desc.contains("readme") {
            Self::TechWriter
        } else if desc.contains("architect")
            || desc.contains("design")
            || desc.contains("structure")
        {
            Self::Architect
        } else if desc.contains("performance")
            || desc.contains("optimize")
            || desc.contains("speed")
        {
            Self::PerformanceEngineer
        } else if desc.contains("review") {
            Self::CodeReviewer
        } else if desc.contains("implement") || desc.contains("create") || desc.contains("add") {
            Self::Engineer
        } else if desc.contains("priorit") || desc.contains("plan") {
            Self::ProductManager
        } else {
            Self::Generalist
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_roles_have_config() {
        for role in HiveRole::all() {
            let config = role.config();
            assert!(!config.name.is_empty());
            assert!(!config.description.is_empty());
            assert!(!config.tools.is_empty());
        }
    }

    #[test]
    fn test_role_task_matching() {
        assert_eq!(
            HiveRole::for_task("Fix security vulnerability"),
            HiveRole::SecurityAuditor
        );
        assert_eq!(
            HiveRole::for_task("Write unit tests"),
            HiveRole::QualityAssurance
        );
        assert_eq!(
            HiveRole::for_task("Update README documentation"),
            HiveRole::TechWriter
        );
        assert_eq!(
            HiveRole::for_task("Design system architecture"),
            HiveRole::Architect
        );
        assert_eq!(
            HiveRole::for_task("Optimize database queries"),
            HiveRole::PerformanceEngineer
        );
        assert_eq!(
            HiveRole::for_task("Code review changes"),
            HiveRole::CodeReviewer
        );
        assert_eq!(
            HiveRole::for_task("Implement new feature"),
            HiveRole::Engineer
        );
        assert_eq!(
            HiveRole::for_task("Prioritize backlog"),
            HiveRole::ProductManager
        );
        assert_eq!(HiveRole::for_task("Something random"), HiveRole::Generalist);
    }

    #[test]
    fn test_role_capabilities() {
        let engineer = HiveRole::Engineer.config();
        assert!(engineer.capabilities.can_write_code);
        assert!(!engineer.capabilities.can_review_code);

        let reviewer = HiveRole::CodeReviewer.config();
        assert!(!reviewer.capabilities.can_write_code);
        assert!(reviewer.capabilities.can_review_code);
    }

    #[test]
    fn test_role_display() {
        assert_eq!(HiveRole::SecurityAuditor.to_string(), "Security Auditor");
        assert_eq!(HiveRole::QualityAssurance.to_string(), "QA");
    }

    #[test]
    fn test_all_roles_count() {
        assert_eq!(HiveRole::all().len(), 9);
    }

    #[test]
    fn test_role_priority_ordering() {
        let pm = HiveRole::ProductManager.config();
        let engineer = HiveRole::Engineer.config();

        assert!(
            pm.priority < engineer.priority,
            "PM debe tener mayor prioridad"
        );
    }
}
