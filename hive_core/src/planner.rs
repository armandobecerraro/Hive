//! Planificador explícito antes de ejecutar.
//!
//! Genera un plan paso a paso, lo revisa, y luego ejecuta cada paso.
//! Inspirado en Devin que genera un plan, Copilot Workspace que planifica cambios,
//! y OpenHands que tiene un paso de planificación explícito.

use serde::{Deserialize, Serialize};

/// Un paso del plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: usize,
    pub description: String,
    pub action_type: PlanAction,
    pub target_files: Vec<String>,
    pub dependencies: Vec<usize>,
    pub completed: bool,
    pub result: Option<String>,
}

/// Tipo de acción de un paso.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlanAction {
    Analyze,
    CreateFile,
    ModifyFile,
    DeleteFile,
    RunCommand,
    RunTest,
    Commit,
    Review,
}

/// Plan completo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub goal: String,
    pub steps: Vec<PlanStep>,
    pub created_at: i64,
    pub current_step: usize,
    pub status: PlanStatus,
}

/// Estado del plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PlanStatus {
    Draft,
    Approved,
    InProgress,
    Completed,
    Failed,
}

/// Generador de planes.
pub struct Planner;

impl Planner {
    /// Genera un plan a partir de una descripción de tarea.
    pub fn generate_plan(goal: &str, context: &str) -> ExecutionPlan {
        let mut steps = Vec::new();
        let mut id = 1;

        // Paso 1: Analizar el código existente
        steps.push(PlanStep {
            id,
            description: "Analizar el código y contexto existente".into(),
            action_type: PlanAction::Analyze,
            target_files: vec![],
            dependencies: vec![],
            completed: false,
            result: None,
        });
        id += 1;

        // Detectar si es creación o modificación
        let is_creation =
            goal.contains("crear") || goal.contains("nuevo") || goal.contains("create");
        let is_modification = goal.contains("modificar")
            || goal.contains("arreglar")
            || goal.contains("fix")
            || goal.contains("mejorar");

        if is_creation {
            steps.push(PlanStep {
                id,
                description: format!("Crear archivos necesarios para: {goal}"),
                action_type: PlanAction::CreateFile,
                target_files: vec![],
                dependencies: vec![1],
                completed: false,
                result: None,
            });
            id += 1;
        }

        if is_modification {
            steps.push(PlanStep {
                id,
                description: format!("Modificar archivos según: {goal}"),
                action_type: PlanAction::ModifyFile,
                target_files: vec![],
                dependencies: vec![1],
                completed: false,
                result: None,
            });
            id += 1;
        }

        // Paso: Ejecutar tests
        steps.push(PlanStep {
            id,
            description: "Ejecutar tests para verificar cambios".into(),
            action_type: PlanAction::RunTest,
            target_files: vec![],
            dependencies: vec![id - 1],
            completed: false,
            result: None,
        });
        id += 1;

        // Paso: Revisión
        steps.push(PlanStep {
            id,
            description: "Revisar los cambios realizados".into(),
            action_type: PlanAction::Review,
            target_files: vec![],
            dependencies: vec![id - 1],
            completed: false,
            result: None,
        });
        id += 1;

        // Paso: Commit
        steps.push(PlanStep {
            id,
            description: "Commitear los cambios".into(),
            action_type: PlanAction::Commit,
            target_files: vec![],
            dependencies: vec![id - 1],
            completed: false,
            result: None,
        });

        ExecutionPlan {
            goal: goal.to_string(),
            steps,
            created_at: chrono::Utc::now().timestamp(),
            current_step: 0,
            status: PlanStatus::Draft,
        }
    }

    /// Aprueba el plan para ejecución.
    pub fn approve_plan(plan: &mut ExecutionPlan) {
        plan.status = PlanStatus::Approved;
    }

    /// Marca un paso como completado.
    pub fn complete_step(plan: &mut ExecutionPlan, step_id: usize, result: &str) {
        if let Some(step) = plan.steps.iter_mut().find(|s| s.id == step_id) {
            step.completed = true;
            step.result = Some(result.to_string());
        }
        plan.current_step = step_id;
    }

    /// Obtiene el siguiente paso pendiente.
    pub fn next_step(plan: &ExecutionPlan) -> Option<&PlanStep> {
        plan.steps.iter().find(|s| {
            !s.completed && {
                s.dependencies
                    .iter()
                    .all(|dep| plan.steps.iter().any(|ps| ps.id == *dep && ps.completed))
            }
        })
    }

    /// Verifica si el plan está completo.
    pub fn is_complete(plan: &ExecutionPlan) -> bool {
        plan.steps.iter().all(|s| s.completed)
    }

    /// Genera un resumen del plan para mostrar al usuario.
    pub fn format_plan(plan: &ExecutionPlan) -> String {
        let mut output = format!("=== Plan: {} ===\n\n", plan.goal);
        for step in &plan.steps {
            let status = if step.completed { "✅" } else { "⏳" };
            let action = match step.action_type {
                PlanAction::Analyze => "🔍",
                PlanAction::CreateFile => "📄",
                PlanAction::ModifyFile => "✏️",
                PlanAction::DeleteFile => "🗑️",
                PlanAction::RunCommand => "⚙️",
                PlanAction::RunTest => "🧪",
                PlanAction::Commit => "💾",
                PlanAction::Review => "👁️",
            };
            output.push_str(&format!(
                "{status} {action} {}. {}\n",
                step.id, step.description
            ));
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_plan_has_steps() {
        let plan = Planner::generate_plan("crear un módulo de tests", "contexto");
        assert!(!plan.steps.is_empty());
        assert_eq!(plan.status, PlanStatus::Draft);
    }

    #[test]
    fn approve_plan_changes_status() {
        let mut plan = Planner::generate_plan("test", "");
        Planner::approve_plan(&mut plan);
        assert_eq!(plan.status, PlanStatus::Approved);
    }

    #[test]
    fn complete_step_marks_done() {
        let mut plan = Planner::generate_plan("fix bug", "");
        Planner::complete_step(&mut plan, 1, "análisis completado");
        assert!(plan.steps[0].completed);
    }

    #[test]
    fn next_step_respects_dependencies() {
        let plan = Planner::generate_plan("test", "");
        let next = Planner::next_step(&plan);
        assert!(next.is_some());
        assert_eq!(next.unwrap().id, 1); // Primer paso sin dependencias
    }

    #[test]
    fn format_plan_produces_output() {
        let plan = Planner::generate_plan("test goal", "");
        let formatted = Planner::format_plan(&plan);
        assert!(formatted.contains("test goal"));
    }
}
