//! Evaluación continua del propio Hive (CI/CD meta-evaluación).
//!
//! Ejecuta E2E tests con repos reales y mide métricas de efectividad del sistema.
//! Regression testing: compara resultados entre versiones de Hive.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

/// Caso de evaluación E2E.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCase {
    pub name: String,
    pub description: String,
    pub setup_commands: Vec<String>,
    pub hive_request: Option<String>,
    pub expected_artifacts: Vec<String>,
    pub expected_test_pass: bool,
    pub timeout_secs: u64,
}

/// Resultado de un caso de evaluación.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub case_name: String,
    pub passed: bool,
    pub duration_secs: u64,
    pub artifacts_found: Vec<String>,
    pub artifacts_missing: Vec<String>,
    pub tests_passed: bool,
    pub error: Option<String>,
    pub metrics: EvalMetrics,
}

/// Métricas de un caso.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EvalMetrics {
    pub files_created: usize,
    pub files_modified: usize,
    pub lines_added: usize,
    pub commits_made: usize,
    pub workers_spawned: usize,
}

/// Reporte completo de evaluación.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    pub timestamp: String,
    pub total_cases: usize,
    pub passed: usize,
    pub failed: usize,
    pub pass_rate: f64,
    pub total_duration_secs: u64,
    pub results: Vec<EvalResult>,
    pub regression: Option<RegressionReport>,
}

/// Reporte de regresión comparando con ejecución anterior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionReport {
    pub previous_pass_rate: f64,
    pub current_pass_rate: f64,
    pub delta: f64,
    pub regressions: Vec<String>,
    pub improvements: Vec<String>,
}

/// Evaluador E2E del propio Hive.
pub struct CicdEvaluator {
    cases: Vec<EvalCase>,
    history_path: PathBuf,
}

impl CicdEvaluator {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            cases: Self::default_cases(),
            history_path: repo_root.join(".hive").join("eval_history.json"),
        }
    }

    fn default_cases() -> Vec<EvalCase> {
        vec![
            EvalCase {
                name: "greenfield_rust".into(),
                description: "Crear proyecto Rust desde cero".into(),
                setup_commands: vec![],
                hive_request: Some("Crear un proyecto Rust básico".into()),
                expected_artifacts: vec![
                    "Cargo.toml".into(),
                    "src/main.rs".into(),
                    "README.md".into(),
                ],
                expected_test_pass: true,
                timeout_secs: 120,
            },
            EvalCase {
                name: "existing_repo_improve".into(),
                description: "Mejorar un repo existente".into(),
                setup_commands: vec![
                    "echo '[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"' > Cargo.toml"
                        .into(),
                    "mkdir -p src && echo 'fn main() {}' > src/main.rs".into(),
                ],
                hive_request: None,
                expected_artifacts: vec!["docs/HIVE_IMPROVEMENTS.md".into()],
                expected_test_pass: true,
                timeout_secs: 120,
            },
            EvalCase {
                name: "python_scaffold".into(),
                description: "Scaffolding de proyecto Python".into(),
                setup_commands: vec![],
                hive_request: Some("Crear un proyecto Python".into()),
                expected_artifacts: vec!["main.py".into()],
                expected_test_pass: true,
                timeout_secs: 60,
            },
            EvalCase {
                name: "feedback_integration".into(),
                description: "Integración de feedback del cliente".into(),
                setup_commands: vec![
                    "echo '[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"' > Cargo.toml"
                        .into(),
                    "mkdir -p src && echo 'fn main() {}' > src/main.rs".into(),
                    "echo 'Añade un comentario al main.' > hive.client_feedback.md".into(),
                ],
                hive_request: None,
                expected_artifacts: vec![".hive/client_feedback_archive".into()],
                expected_test_pass: true,
                timeout_secs: 120,
            },
        ]
    }

    /// Ejecuta todos los casos de evaluación.
    pub async fn run_all(&self, cfg: &crate::config::HiveConfig) -> Result<EvalReport> {
        info!(cases = self.cases.len(), "iniciando evaluación E2E");
        let start = std::time::Instant::now();

        let mut results = Vec::new();
        for case in &self.cases {
            let result = self.run_case(case, cfg).await;
            info!(
                case = %case.name,
                passed = result.passed,
                "caso de evaluación completado"
            );
            results.push(result);
        }

        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.len() - passed;
        let pass_rate = if results.is_empty() {
            0.0
        } else {
            (passed as f64 / results.len() as f64) * 100.0
        };

        let report = EvalReport {
            timestamp: chrono::Utc::now().to_rfc3339(),
            total_cases: results.len(),
            passed,
            failed,
            pass_rate,
            total_duration_secs: start.elapsed().as_secs(),
            results,
            regression: self.check_regression(pass_rate),
        };

        self.save_history(&report)?;
        Ok(report)
    }

    async fn run_case(&self, case: &EvalCase, cfg: &crate::config::HiveConfig) -> EvalResult {
        let start = std::time::Instant::now();
        let tmp_dir = std::env::temp_dir().join(format!("hive_eval_{}", case.name));
        let _ = std::fs::create_dir_all(&tmp_dir);

        // Ejecutar comandos de setup
        for cmd in &case.setup_commands {
            let _ = std::process::Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .current_dir(&tmp_dir)
                .output();
        }

        // Escribir hive.request.json si aplica
        if let Some(ref request_text) = case.hive_request {
            let req = crate::request::HiveRequest {
                title: request_text.clone(),
                description: request_text.clone(),
                ..Default::default()
            };
            let _ = std::fs::write(
                tmp_dir.join("hive.request.json"),
                serde_json::to_string(&req).unwrap_or_default(),
            );
        }

        // Ejecutar ciclo de Hive
        let timeout = std::time::Duration::from_secs(case.timeout_secs);
        let cycle_result =
            tokio::time::timeout(timeout, crate::run_queen_cycle(tmp_dir.clone(), cfg)).await;

        let success = matches!(&cycle_result, Ok(Ok(_)));

        // Verificar artefactos esperados
        let mut found = Vec::new();
        let mut missing = Vec::new();
        for artifact in &case.expected_artifacts {
            if tmp_dir.join(artifact).exists() {
                found.push(artifact.clone());
            } else {
                missing.push(artifact.clone());
            }
        }

        // Contar métricas
        let files = std::fs::read_dir(&tmp_dir)
            .map(|entries| entries.count())
            .unwrap_or(0);

        EvalResult {
            case_name: case.name.clone(),
            passed: success && missing.is_empty(),
            duration_secs: start.elapsed().as_secs(),
            artifacts_found: found,
            artifacts_missing: missing,
            tests_passed: success,
            error: match cycle_result {
                Ok(Err(e)) => Some(e.to_string()),
                Err(_) => Some("timeout".into()),
                _ => None,
            },
            metrics: EvalMetrics {
                files_created: files,
                ..Default::default()
            },
        }
    }

    fn check_regression(&self, current_pass_rate: f64) -> Option<RegressionReport> {
        let history = self.load_history().ok()?;
        if history.is_empty() {
            return None;
        }

        let previous = history.last()?;
        let delta = current_pass_rate - previous.pass_rate;

        let mut regressions = Vec::new();
        let mut improvements = Vec::new();

        for prev_result in &previous.results {
            let current = self.cases.iter().find(|c| c.name == prev_result.case_name);
            if let Some(_current) = current {
                // Comparar: si antes pasaba y ahora no, es regresión
                if prev_result.passed {
                    regressions.push(format!(
                        "{}: pasó antes, falla ahora",
                        prev_result.case_name
                    ));
                } else {
                    improvements.push(format!(
                        "{}: falló antes, ahora evaluar",
                        prev_result.case_name
                    ));
                }
            }
        }

        Some(RegressionReport {
            previous_pass_rate: previous.pass_rate,
            current_pass_rate,
            delta,
            regressions,
            improvements,
        })
    }

    fn load_history(&self) -> Result<Vec<EvalReport>> {
        if !self.history_path.exists() {
            return Ok(Vec::new());
        }
        let raw = std::fs::read_to_string(&self.history_path)?;
        let reports: Vec<EvalReport> = serde_json::from_str(&raw)?;
        Ok(reports)
    }

    fn save_history(&self, report: &EvalReport) -> Result<()> {
        if let Some(parent) = self.history_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut history = self.load_history().unwrap_or_default();
        history.push(report.clone());

        // Mantener solo los últimos 20
        if history.len() > 20 {
            history = history[history.len() - 20..].to_vec();
        }

        let json = serde_json::to_string_pretty(&history)?;
        std::fs::write(&self.history_path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluator_tiene_casos_por_defecto() {
        let cases = CicdEvaluator::default_cases();
        assert!(!cases.is_empty());
    }

    #[test]
    fn evaluator_carga_casos() {
        let tmp = tempfile::tempdir().unwrap();
        let evaluator = CicdEvaluator::new(tmp.path());
        assert_eq!(evaluator.cases.len(), 4);
    }
}
