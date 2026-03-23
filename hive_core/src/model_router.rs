//! # Model Router - Selección Inteligente de Modelos
//!
//! Inspirado en CrewAI y LangGraph: asigna **modelos diferentes según el tipo
//! de tarea**, reduciendo costos 60-80% sin sacrificar calidad donde importa.
//!
//! ## Estrategia
//! | Tarea | Modelo sugerido | Razón |
//! |-------|----------------|-------|
//! | Security audit | GPT-4o / Claude Opus | Máxima precisión |
//! | Code generation | Codellama / GPT-4 | Balance costo/calidad |
//! | Documentation | Modelo ligero | No necesita razonamiento profundo |
//! | Test generation | Modelo medio | Buena estructura |

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tipo de tarea que el modelo debe realizar
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskType {
    /// Análisis de seguridad (requiere máxima precisión)
    SecurityAudit,
    /// Generación de código
    CodeGeneration,
    /// Revisión de código
    CodeReview,
    /// Generación de tests
    TestGeneration,
    /// Documentación
    Documentation,
    /// Análisis de deuda técnica
    DebtAnalysis,
    /// Refactoring
    Refactoring,
    /// Respuesta genérica
    General,
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SecurityAudit => write!(f, "security_audit"),
            Self::CodeGeneration => write!(f, "code_generation"),
            Self::CodeReview => write!(f, "code_review"),
            Self::TestGeneration => write!(f, "test_generation"),
            Self::Documentation => write!(f, "documentation"),
            Self::DebtAnalysis => write!(f, "debt_analysis"),
            Self::Refactoring => write!(f, "refactoring"),
            Self::General => write!(f, "general"),
        }
    }
}

/// Tiers de modelos disponibles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelTier {
    /// Máxima capacidad (GPT-4o, Claude Opus)
    Premium,
    /// Balance (GPT-4, Claude Sonnet, Codellama 70B)
    Standard,
    /// Económico (GPT-3.5, modelos 7B)
    Economic,
}

/// Configuración de un modelo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    pub tier: ModelTier,
    pub context_window: u32,
    pub cost_per_1k_tokens: f32,
    pub supports_functions: bool,
}

/// Resultado del routing
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub task_type: TaskType,
    pub model: ModelConfig,
    pub reason: String,
}

/// Router de modelos
///
/// Selecciona el modelo óptimo basándose en el tipo de tarea,
/// presupuesto disponible y preferencias de calidad.
pub struct ModelRouter {
    models: HashMap<TaskType, ModelConfig>,
    default_model: ModelConfig,
    budget_remaining: f32,
    total_spent: f32,
}

impl ModelRouter {
    /// Crea un router con modelos por defecto
    pub fn new() -> Self {
        let mut models = HashMap::new();

        // Security: modelo premium
        models.insert(
            TaskType::SecurityAudit,
            ModelConfig {
                name: "gpt-4o".into(),
                tier: ModelTier::Premium,
                context_window: 128_000,
                cost_per_1k_tokens: 0.015,
                supports_functions: true,
            },
        );

        // Code generation: modelo estándar
        models.insert(
            TaskType::CodeGeneration,
            ModelConfig {
                name: "codellama:70b".into(),
                tier: ModelTier::Standard,
                context_window: 16_384,
                cost_per_1k_tokens: 0.003,
                supports_functions: false,
            },
        );

        // Code review: modelo estándar
        models.insert(
            TaskType::CodeReview,
            ModelConfig {
                name: "gpt-4".into(),
                tier: ModelTier::Standard,
                context_window: 8_192,
                cost_per_1k_tokens: 0.006,
                supports_functions: true,
            },
        );

        // Test generation: modelo estándar
        models.insert(
            TaskType::TestGeneration,
            ModelConfig {
                name: "claude-3-sonnet".into(),
                tier: ModelTier::Standard,
                context_window: 200_000,
                cost_per_1k_tokens: 0.003,
                supports_functions: true,
            },
        );

        // Documentation: modelo económico
        models.insert(
            TaskType::Documentation,
            ModelConfig {
                name: "llama3:8b".into(),
                tier: ModelTier::Economic,
                context_window: 8_192,
                cost_per_1k_tokens: 0.0001,
                supports_functions: false,
            },
        );

        // Debt analysis: modelo económico
        models.insert(
            TaskType::DebtAnalysis,
            ModelConfig {
                name: "llama3:8b".into(),
                tier: ModelTier::Economic,
                context_window: 8_192,
                cost_per_1k_tokens: 0.0001,
                supports_functions: false,
            },
        );

        // Refactoring: modelo estándar
        models.insert(
            TaskType::Refactoring,
            ModelConfig {
                name: "gpt-4".into(),
                tier: ModelTier::Standard,
                context_window: 8_192,
                cost_per_1k_tokens: 0.006,
                supports_functions: true,
            },
        );

        let default_model = ModelConfig {
            name: "codellama:7b".into(),
            tier: ModelTier::Economic,
            context_window: 4_096,
            cost_per_1k_tokens: 0.0001,
            supports_functions: false,
        };

        Self {
            models,
            default_model,
            budget_remaining: 10.0, // $10 por defecto
            total_spent: 0.0,
        }
    }

    /// Crea un router con configuración personalizada
    pub fn with_models(models: HashMap<TaskType, ModelConfig>, default: ModelConfig) -> Self {
        Self {
            models,
            default_model: default,
            budget_remaining: 10.0,
            total_spent: 0.0,
        }
    }

    /// Selecciona el modelo para un tipo de tarea
    pub fn route(&self, task_type: TaskType) -> RoutingDecision {
        let model = self.models.get(&task_type).unwrap_or(&self.default_model);

        // Si el presupuesto es bajo, downgrade a económico
        let final_model = if self.budget_remaining < 0.50 {
            ModelConfig {
                name: format!("{} (budget-limited)", model.name),
                tier: ModelTier::Economic,
                ..model.clone()
            }
        } else {
            model.clone()
        };

        let reason = match final_model.tier {
            ModelTier::Premium => "Tarea crítica requiere máxima precisión".into(),
            ModelTier::Standard => "Balance óptimo de costo y calidad".into(),
            ModelTier::Economic => "Tarea simple, modelo económico suficiente".into(),
        };

        RoutingDecision {
            task_type,
            model: final_model,
            reason,
        }
    }

    /// Registra gasto de tokens
    pub fn record_usage(&mut self, tokens: u32, cost_per_1k: f32) {
        let cost = (tokens as f32 / 1000.0) * cost_per_1k;
        self.budget_remaining -= cost;
        self.total_spent += cost;
    }

    /// Establece presupuesto
    pub fn set_budget(&mut self, budget: f32) {
        self.budget_remaining = budget;
    }

    /// Obtiene presupuesto restante
    pub fn budget_remaining(&self) -> f32 {
        self.budget_remaining
    }

    /// Obtiene total gastado
    pub fn total_spent(&self) -> f32 {
        self.total_spent
    }

    /// Lista modelos configurados
    pub fn list_models(&self) -> Vec<(TaskType, &ModelConfig)> {
        self.models.iter().map(|(k, v)| (*k, v)).collect()
    }
}

impl Default for ModelRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_security_uses_premium() {
        let router = ModelRouter::new();
        let decision = router.route(TaskType::SecurityAudit);

        assert_eq!(decision.model.tier, ModelTier::Premium);
        assert!(decision.reason.contains("precisión"));
    }

    #[test]
    fn test_route_documentation_uses_economic() {
        let router = ModelRouter::new();
        let decision = router.route(TaskType::Documentation);

        assert_eq!(decision.model.tier, ModelTier::Economic);
    }

    #[test]
    fn test_budget_limitation() {
        let mut router = ModelRouter::new();
        router.set_budget(0.10); // Presupuesto muy bajo

        // Simular gasto que excede presupuesto
        router.record_usage(5000, 0.02);

        let decision = router.route(TaskType::SecurityAudit);
        // Debe downgrade por presupuesto
        assert!(decision.model.name.contains("budget-limited"));
    }

    #[test]
    fn test_usage_tracking() {
        let mut router = ModelRouter::new();
        let initial_budget = router.budget_remaining();

        router.record_usage(1000, 0.01);

        assert!((router.budget_remaining() - (initial_budget - 0.01)).abs() < 0.001);
        assert!((router.total_spent() - 0.01).abs() < 0.001);
    }

    #[test]
    fn test_custom_models() {
        let mut models = HashMap::new();
        models.insert(
            TaskType::CodeGeneration,
            ModelConfig {
                name: "my-custom-model".into(),
                tier: ModelTier::Premium,
                context_window: 32_000,
                cost_per_1k_tokens: 0.02,
                supports_functions: true,
            },
        );

        let router = ModelRouter::with_models(
            models,
            ModelConfig {
                name: "fallback".into(),
                tier: ModelTier::Economic,
                context_window: 4_096,
                cost_per_1k_tokens: 0.001,
                supports_functions: false,
            },
        );

        let decision = router.route(TaskType::CodeGeneration);
        assert_eq!(decision.model.name, "my-custom-model");

        // TaskType no configurado usa fallback
        let decision = router.route(TaskType::General);
        assert_eq!(decision.model.name, "fallback");
    }

    #[test]
    fn test_all_task_types_have_routing() {
        let router = ModelRouter::new();

        let types = [
            TaskType::SecurityAudit,
            TaskType::CodeGeneration,
            TaskType::CodeReview,
            TaskType::TestGeneration,
            TaskType::Documentation,
            TaskType::DebtAnalysis,
            TaskType::Refactoring,
            TaskType::General,
        ];

        for task_type in types {
            let decision = router.route(task_type);
            assert!(!decision.model.name.is_empty());
            assert!(!decision.reason.is_empty());
        }
    }

    #[test]
    fn test_list_models() {
        let router = ModelRouter::new();
        let models = router.list_models();
        assert!(!models.is_empty());
    }

    #[test]
    fn test_multiple_usage_records() {
        let mut router = ModelRouter::new();
        let initial = router.budget_remaining();

        router.record_usage(1000, 0.01);
        router.record_usage(1000, 0.01);
        router.record_usage(1000, 0.01);

        let expected_spent = 0.03;
        assert!((router.total_spent() - expected_spent).abs() < 0.001);
        assert!((router.budget_remaining() - (initial - expected_spent)).abs() < 0.001);
    }

    #[test]
    fn test_task_type_display() {
        assert_eq!(format!("{}", TaskType::SecurityAudit), "security_audit");
        assert_eq!(format!("{}", TaskType::CodeGeneration), "code_generation");
        assert_eq!(format!("{}", TaskType::General), "general");
    }

    #[test]
    fn test_model_config_serialization() {
        let config = ModelConfig {
            name: "gpt-4".into(),
            tier: ModelTier::Premium,
            context_window: 128_000,
            cost_per_1k_tokens: 0.015,
            supports_functions: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ModelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "gpt-4");
    }

    #[test]
    fn test_routing_decision_structure() {
        let router = ModelRouter::new();
        let decision = router.route(TaskType::CodeReview);
        assert!(!decision.reason.is_empty());
        assert!(!decision.model.name.is_empty());
    }
}
