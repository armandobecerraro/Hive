//! # Prioritizer - Sistema de Priorización Inteligente de Tareas
//!
//! Ordena las tareas del enjambre basándose en:
//! - Severidad de problemas de seguridad
//! - Impacto en la calidad del código
//! - Dependencias entre tareas
//! - Esfuerzo estimado

use crate::discovery::{CodePattern, PatternType, SecurityIssue, Severity, TechnicalDebtHints};
use uuid::Uuid;

/// Score de prioridad para una tarea (mayor = más prioritario)
#[derive(Debug, Clone)]
pub struct PriorityScore {
    pub task_id: Uuid,
    pub score: f32,
    pub reasons: Vec<String>,
}

/// Calculador de prioridades
pub struct Prioritizer {
    /// Peso de problemas de seguridad
    pub security_weight: f32,
    /// Peso de deuda técnica
    pub debt_weight: f32,
    /// Peso de patrones de código problemáticos
    pub pattern_weight: f32,
}

impl Default for Prioritizer {
    fn default() -> Self {
        Self {
            security_weight: 3.0,
            debt_weight: 1.5,
            pattern_weight: 1.0,
        }
    }
}

impl Prioritizer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Prioriza tareas basándose en problemas de seguridad detectados
    pub fn prioritize_security_tasks(&self, issues: &[SecurityIssue]) -> Vec<PriorityScore> {
        let mut scores = Vec::new();

        for issue in issues.iter() {
            let severity_score = match issue.severity {
                Severity::Critical => 100.0,
                Severity::High => 75.0,
                Severity::Medium => 50.0,
                Severity::Low => 25.0,
                Severity::Info => 10.0,
            };

            let category_score = match &issue.category {
                crate::discovery::SecurityCategory::HardcodedSecret => 90.0,
                crate::discovery::SecurityCategory::SqlInjection => 95.0,
                crate::discovery::SecurityCategory::CommandInjection => 85.0,
                crate::discovery::SecurityCategory::UnsafeCode => 60.0,
                crate::discovery::SecurityCategory::WeakCrypto => 70.0,
                crate::discovery::SecurityCategory::PathTraversal => 80.0,
                crate::discovery::SecurityCategory::InsecureDeserialization => 75.0,
                crate::discovery::SecurityCategory::MissingValidation => 40.0,
                crate::discovery::SecurityCategory::ExposedDebug => 55.0,
                crate::discovery::SecurityCategory::OutdatedDependency => 30.0,
            };

            let score = (severity_score + category_score) * self.security_weight;

            scores.push(PriorityScore {
                task_id: Uuid::new_v4(),
                score,
                reasons: vec![
                    format!("Problema de seguridad: {:?}", issue.category),
                    format!("Severidad: {:?}", issue.severity),
                    format!("Archivo: {}", issue.file.display()),
                ],
            });
        }

        scores.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scores
    }

    /// Prioriza tareas basándose en patrones de código problemáticos
    pub fn prioritize_pattern_tasks(&self, patterns: &[CodePattern]) -> Vec<PriorityScore> {
        let mut scores = Vec::new();

        for pattern in patterns {
            let base_score = match &pattern.pattern_type {
                PatternType::LargeFunction => 40.0,
                PatternType::DuplicatedCode => 50.0,
                PatternType::GodClass => 60.0,
                PatternType::DeepNesting => 35.0,
                PatternType::MagicNumber => 20.0,
                PatternType::LongParameterList => 30.0,
                PatternType::EmptyCatch => 45.0,
                PatternType::TodoFixme => 25.0,
            };

            // Más ocurrencias = más prioritario
            let score =
                base_score * (1.0 + (pattern.occurrences as f32).log2()) * self.pattern_weight;

            scores.push(PriorityScore {
                task_id: Uuid::new_v4(),
                score,
                reasons: vec![
                    format!("Patrón: {:?}", pattern.pattern_type),
                    format!("Ocurrencias: {}", pattern.occurrences),
                    pattern.suggestion.clone(),
                ],
            });
        }

        scores.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scores
    }

    /// Prioriza tareas basándose en deuda técnica
    pub fn prioritize_debt_tasks(&self, debt: &TechnicalDebtHints) -> Vec<PriorityScore> {
        let mut scores = Vec::new();

        if debt.missing_readme {
            scores.push(PriorityScore {
                task_id: Uuid::new_v4(),
                score: 15.0 * self.debt_weight,
                reasons: vec!["Falta README.md".to_string()],
            });
        }

        if debt.missing_license {
            scores.push(PriorityScore {
                task_id: Uuid::new_v4(),
                score: 10.0 * self.debt_weight,
                reasons: vec!["Falta LICENSE".to_string()],
            });
        }

        if debt.sparse_tests {
            scores.push(PriorityScore {
                task_id: Uuid::new_v4(),
                score: 35.0 * self.debt_weight,
                reasons: vec!["Tests escasos detectados".to_string()],
            });
        }

        if debt.todo_markers > 5 {
            scores.push(PriorityScore {
                task_id: Uuid::new_v4(),
                score: 20.0 * (debt.todo_markers as f32).log2() * self.debt_weight,
                reasons: vec![format!("{} marcadores TODO/FIXME", debt.todo_markers)],
            });
        }

        if debt.large_files > 0 {
            scores.push(PriorityScore {
                task_id: Uuid::new_v4(),
                score: 15.0 * (debt.large_files as f32) * self.debt_weight,
                reasons: vec![format!("{} archivos grandes (>512KB)", debt.large_files)],
            });
        }

        scores.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scores
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::{SecurityCategory, Severity};

    #[test]
    fn test_security_prioritization() {
        let prioritizer = Prioritizer::new();

        let issues = vec![
            SecurityIssue {
                severity: Severity::Critical,
                category: SecurityCategory::SqlInjection,
                file: std::path::PathBuf::from("db.rs"),
                line: Some(42),
                description: "SQL injection".to_string(),
                suggestion: "Use parametrized queries".to_string(),
            },
            SecurityIssue {
                severity: Severity::Low,
                category: SecurityCategory::OutdatedDependency,
                file: std::path::PathBuf::from("Cargo.toml"),
                line: None,
                description: "Old dependency".to_string(),
                suggestion: "Update".to_string(),
            },
        ];

        let scores = prioritizer.prioritize_security_tasks(&issues);
        assert!(!scores.is_empty());
        // Critical should be first
        assert!(scores[0].score > scores[1].score);
    }

    #[test]
    fn test_pattern_prioritization() {
        let prioritizer = Prioritizer::new();

        let patterns = vec![
            CodePattern {
                pattern_type: PatternType::GodClass,
                occurrences: 5,
                files: vec![std::path::PathBuf::from("big.rs")],
                suggestion: "Split class".to_string(),
            },
            CodePattern {
                pattern_type: PatternType::TodoFixme,
                occurrences: 2,
                files: vec![std::path::PathBuf::from("main.rs")],
                suggestion: "Fix TODOs".to_string(),
            },
        ];

        let scores = prioritizer.prioritize_pattern_tasks(&patterns);
        assert!(!scores.is_empty());
    }

    #[test]
    fn test_debt_prioritization() {
        let prioritizer = Prioritizer::new();

        let debt = TechnicalDebtHints {
            todo_markers: 10,
            large_files: 2,
            missing_readme: true,
            missing_license: true,
            sparse_tests: true,
        };

        let scores = prioritizer.prioritize_debt_tasks(&debt);
        assert!(!scores.is_empty());
        // Tests escasos should be high priority
        assert!(scores
            .iter()
            .any(|s| s.reasons.iter().any(|r| r.contains("escasos"))));
    }
}
