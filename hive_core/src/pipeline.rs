//! Pipeline end-to-end unificado.
//!
//! Este es EL camino obligatorio que usa cada ciclo de Hive:
//! contexto → retrieval → LLM → ediciones → validación → commit
//!
//! Garantiza que NINGÚN ciclo puede saltarse pasos y que se miden
//! métricas y coste por cada etapa. Es lo que diferencia un "motor"
//! de un "producto" listo para producción.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Estado de cada etapa del pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StageStatus {
    Pending,
    Running,
    Success,
    Failed,
    Skipped,
}

/// Resultado de una etapa individual.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    pub stage_name: String,
    pub status: StageStatus,
    pub duration_ms: u64,
    pub tokens_used: usize,
    pub cost_usd: f64,
    pub error: Option<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Reporte completo del pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineReport {
    pub cycle_id: String,
    pub stages: Vec<StageResult>,
    pub total_duration_ms: u64,
    pub total_tokens: usize,
    pub total_cost_usd: f64,
    pub success: bool,
    pub files_changed: Vec<String>,
    pub patches_applied: usize,
    pub tests_passed: bool,
    pub commit_hash: Option<String>,
}

/// Métricas acumuladas del sistema.
#[derive(Debug, Clone, Serialize, Default)]
pub struct PipelineMetrics {
    pub total_cycles: usize,
    pub successful_cycles: usize,
    pub failed_cycles: usize,
    pub total_tokens: usize,
    pub total_cost_usd: f64,
    pub avg_duration_ms: u64,
    pub success_rate: f64,
    pub cost_per_success: f64,
}

/// Etapas del pipeline (orden obligatorio).
const STAGES: &[&str] = &[
    "resolve_request", // 1. Resolver hive.request.json
    "scan_context",    // 2. Escanear archivos del repo
    "chunk_and_rank",  // 3. Chunks + relevancia
    "retrieve_memory", // 4. Buscar en memoria RAG
    "select_model",    // 5. Seleccionar modelo óptimo
    "generate_plan",   // 6. Generar plan de ejecución
    "llm_call",        // 7. Llamar al LLM
    "parse_response",  // 8. Parsear respuesta (edits)
    "apply_edits",     // 9. Aplicar ediciones
    "validate",        // 10. Compilar + tests
    "review",          // 11. Code review automático
    "commit",          // 12. Commit semántico
    "record_metrics",  // 13. Registrar métricas
];

/// Ejecutor del pipeline unificado.
pub struct PipelineExecutor {
    stages: Vec<StageResult>,
    cycle_id: String,
    start_time: std::time::Instant,
}

impl PipelineExecutor {
    pub fn new() -> Self {
        Self {
            stages: Vec::new(),
            cycle_id: uuid::Uuid::new_v4().to_string(),
            start_time: std::time::Instant::now(),
        }
    }

    /// Ejecuta una etapa y registra su resultado.
    pub fn execute_stage<F>(
        &mut self,
        stage_name: &str,
        tokens: usize,
        cost: f64,
        action: F,
    ) -> Result<(), String>
    where
        F: FnOnce() -> Result<std::collections::HashMap<String, String>, String>,
    {
        let start = std::time::Instant::now();

        let (status, error, metadata) = match action() {
            Ok(meta) => (StageStatus::Success, None, meta),
            Err(e) => (
                StageStatus::Failed,
                Some(e),
                std::collections::HashMap::new(),
            ),
        };
        let failed = status == StageStatus::Failed;

        self.stages.push(StageResult {
            stage_name: stage_name.into(),
            status,
            duration_ms: start.elapsed().as_millis() as u64,
            tokens_used: tokens,
            cost_usd: cost,
            error,
            metadata,
        });

        if failed {
            Err(format!("etapa '{stage_name}' falló"))
        } else {
            Ok(())
        }
    }

    /// Genera el reporte final.
    pub fn finalize(
        &self,
        files_changed: Vec<String>,
        patches: usize,
        tests_passed: bool,
        commit: Option<String>,
    ) -> PipelineReport {
        PipelineReport {
            cycle_id: self.cycle_id.clone(),
            stages: self.stages.clone(),
            total_duration_ms: self.start_time.elapsed().as_millis() as u64,
            total_tokens: self.stages.iter().map(|s| s.tokens_used).sum(),
            total_cost_usd: self.stages.iter().map(|s| s.cost_usd).sum(),
            success: self.stages.iter().all(|s| s.status != StageStatus::Failed),
            files_changed,
            patches_applied: patches,
            tests_passed,
            commit_hash: commit,
        }
    }

    /// Verifica que TODAS las etapas obligatorias se completaron.
    pub fn validate_completeness(&self) -> Result<(), Vec<String>> {
        let completed: std::collections::HashSet<&str> = self
            .stages
            .iter()
            .filter(|s| s.status == StageStatus::Success)
            .map(|s| s.stage_name.as_str())
            .collect();

        let missing: Vec<String> = STAGES
            .iter()
            .filter(|s| !completed.contains(*s))
            .map(|s| s.to_string())
            .collect();

        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }

    /// Guarda el reporte en disco.
    pub fn save_report(repo_root: &Path, report: &PipelineReport) -> Result<(), String> {
        let dir = repo_root.join(".hive").join("pipeline_reports");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = dir.join(format!("{}.json", report.cycle_id));
        let json = serde_json::to_string_pretty(report).map_err(|e| e.to_string())?;
        std::fs::write(&file, json).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Carga métricas acumuladas de reportes previos.
    pub fn load_metrics(repo_root: &Path) -> PipelineMetrics {
        let dir = repo_root.join(".hive").join("pipeline_reports");
        if !dir.exists() {
            return PipelineMetrics::default();
        }

        let mut metrics = PipelineMetrics::default();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(report) = serde_json::from_str::<PipelineReport>(&content) {
                        metrics.total_cycles += 1;
                        if report.success {
                            metrics.successful_cycles += 1;
                        } else {
                            metrics.failed_cycles += 1;
                        }
                        metrics.total_tokens += report.total_tokens;
                        metrics.total_cost_usd += report.total_cost_usd;
                    }
                }
            }
        }

        if metrics.total_cycles > 0 {
            metrics.success_rate =
                metrics.successful_cycles as f64 / metrics.total_cycles as f64 * 100.0;
            if metrics.successful_cycles > 0 {
                metrics.cost_per_success =
                    metrics.total_cost_usd / metrics.successful_cycles as f64;
            }
        }
        metrics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_stages_defined() {
        assert!(STAGES.len() >= 12);
        assert!(STAGES.contains(&"llm_call"));
        assert!(STAGES.contains(&"validate"));
    }

    #[test]
    fn executor_records_stages() {
        let mut exec = PipelineExecutor::new();
        exec.execute_stage("resolve_request", 0, 0.0, || {
            Ok(std::collections::HashMap::new())
        })
        .unwrap();
        exec.execute_stage("scan_context", 0, 0.0, || {
            Ok(std::collections::HashMap::new())
        })
        .unwrap();
        assert_eq!(exec.stages.len(), 2);
    }

    #[test]
    fn executor_detects_failure() {
        let mut exec = PipelineExecutor::new();
        exec.execute_stage("llm_call", 100, 0.01, || Err("API timeout".into()))
            .unwrap_err();
        assert_eq!(exec.stages[0].status, StageStatus::Failed);
    }

    #[test]
    fn validate_completeness_detects_missing() {
        let mut exec = PipelineExecutor::new();
        for stage in &["resolve_request", "scan_context"] {
            exec.execute_stage(stage, 0, 0.0, || Ok(std::collections::HashMap::new()))
                .unwrap();
        }
        let result = exec.validate_completeness();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains(&"llm_call".to_string()));
    }

    #[test]
    fn finalize_produces_report() {
        let mut exec = PipelineExecutor::new();
        exec.execute_stage("resolve_request", 10, 0.001, || {
            Ok(std::collections::HashMap::new())
        })
        .unwrap();
        let report = exec.finalize(vec!["main.rs".into()], 1, true, Some("abc123".into()));
        assert!(report.success);
        assert_eq!(report.total_tokens, 10);
    }

    #[test]
    fn metrics_accumulate() {
        let tmp = tempfile::tempdir().unwrap();
        let mut exec = PipelineExecutor::new();
        exec.execute_stage("test", 0, 0.0, || Ok(std::collections::HashMap::new()))
            .unwrap();
        let report = exec.finalize(vec![], 0, true, None);
        PipelineExecutor::save_report(tmp.path(), &report).unwrap();
        let metrics = PipelineExecutor::load_metrics(tmp.path());
        assert_eq!(metrics.total_cycles, 1);
        assert_eq!(metrics.successful_cycles, 1);
    }
}
