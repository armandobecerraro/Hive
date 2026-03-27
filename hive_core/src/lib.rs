//! The Hive — núcleo de orquestación (colonización, ADN, obreras, Consejo).

// Módulos core
pub mod agent;
pub mod config;
pub mod council;
pub mod discovery;
pub mod git_manager;
pub mod orchestrator;
pub mod request;
pub mod resource_monitor;
pub mod state;
pub mod work;

// Módulos de mejora del enjambre
pub mod scaffold_validate;
pub mod sequential_merger;

// --- Mejoras 2026 (siempre disponibles, sin feature) ---
pub mod cicd_eval;
pub mod memory; // Memoria persistente RAG
pub mod multi_repo; // Soporte multi-repo
pub mod swebench; // SWE-bench benchmark

// --- Mejoras 2026 (requieren feature "multiagent") ---
#[cfg(feature = "multiagent")]
pub mod a2a; // Agent-to-Agent protocol
#[cfg(feature = "multiagent")]
pub mod github_api; // Integración GitHub API real
#[cfg(feature = "multiagent")]
pub mod mcp; // Model Context Protocol
#[cfg(feature = "multiagent")]
pub mod pair_programming; // WebSocket pair programming
#[cfg(feature = "multiagent")]
pub mod sandbox; // Sandbox Docker aislado por obrera
#[cfg(feature = "multiagent")]
pub mod task_dag; // DAG de tareas con paralelismo
#[cfg(feature = "multiagent")]
pub mod telemetry; // OpenTelemetry observabilidad
#[cfg(feature = "multiagent")]
pub mod wasm_plugins; // Plugins WASM

// Módulos multi-agente legacy (requieren feature "multiagent")
#[cfg(feature = "multiagent")]
pub mod blackboard;
#[cfg(feature = "multiagent")]
pub mod brain;
#[cfg(feature = "multiagent")]
pub mod worker;

// --- Módulos P0 Críticos (siempre disponibles) ---
pub mod context_window; // Gestión de ventana de contexto
pub mod conversation; // Historial de conversación
pub mod edit_engine; // Edición basada en diffs
pub mod embeddings; // Vector store RAG
pub mod multi_model; // Soporte multi-modelo LLM
pub mod planner; // Planificación explícita
pub mod streaming; // Real-time streaming de LLM
pub mod validator; // Validación de resultados

// --- Módulos P1 Alto impacto (siempre disponibles) ---
pub mod browser; // Browser automation
pub mod checkpoints; // Human-in-the-loop checkpoints
pub mod cost_tracker; // Cost tracking de LLM
pub mod error_recovery; // Error recovery avanzado
pub mod fs_watcher; // File system watcher
pub mod retry; // Retry con cambio de estrategia
pub mod terminal; // Terminal interactivo
pub mod test_generator;
pub mod undo; // Undo/rollback granular // Generación automática de tests

// --- Módulos P2 Diferenciadores (siempre disponibles) ---
pub mod ast_analyzer; // AST-based code understanding
pub mod changelog;
pub mod code_review; // Code review automático
pub mod deps_analysis; // Análisis de dependencias
pub mod semantic_commit; // Generación semántica de commits // Generación de changelog

// --- Módulos P3 Infraestructura (siempre disponibles) ---
pub mod api_spec;
pub mod config_validator; // Validación estricta de config
pub mod health; // Health checks
pub mod llm_cache; // Caching de respuestas LLM
pub mod metrics; // Exportación de métricas
pub mod rate_limiter; // Rate limiting para LLM
pub mod shutdown; // Graceful shutdown // REST API spec

// --- Módulos P4 Seguridad (siempre disponibles) ---
pub mod audit_log; // Audit logging
pub mod conflict_resolver;
pub mod secrets; // Secret management seguro
pub mod supply_chain; // Supply chain security // Resolución de conflictos

// --- Módulos P5 UX (siempre disponibles) ---
pub mod doc_gen; // Generación de documentación
pub mod profiler; // Performance profiling

// --- Módulos P6 Testing (siempre disponibles) ---
pub mod fuzzer;
pub mod mutation_test; // Mutation testing // Fuzzing

// --- Módulos P7 Estándares (siempre disponibles) ---
pub mod adr; // Architecture Decision Records
pub mod commit_validator; // Validación de conventional commits
pub mod reuse; // REUSE compliance checker

// --- Módulos UX/Ecosistema (siempre disponibles) ---
pub mod dashboard; // Web dashboard spec y datos
pub mod hot_reload; // Hot reload de configuración
pub mod marketplace; // Plugin marketplace
pub mod notifications; // Slack/Discord notificaciones
pub mod observability;
pub mod rbac; // Role-Based Access Control
pub mod snapshots; // Snapshot testing
pub mod templates; // Templates/presets por stack
pub mod tui; // Terminal UI con paneles y progreso
pub mod webhooks; // Webhook server GitHub/GitLab // Observabilidad completa

// --- Módulos Sandbox/Security (siempre disponibles) ---
pub mod image_scanner; // Container image scanning
pub mod network_policy; // Network policies para sandbox

// --- Módulos de producto internacional (siempre disponibles) ---
pub mod llm_backend; // Backend de modelo robusto
pub mod pipeline; // Pipeline end-to-end unificado
pub mod product; // CLI/TUI producto mínimo
pub mod threat_model; // Modelo de amenazas y seguridad

use anyhow::Result;
use config::HiveConfig;
use discovery::RepositoryAnalyzer;
use orchestrator::HiveOrchestrator;
use std::path::{Path, PathBuf};
use tracing::info;

/// Borra en la **raíz** del repo los `.hive_worker_<uuid>.md` (formato antiguo; el actual es `.hive/worker_artifact.md`).
/// Devuelve cuántos ficheros se eliminaron del disco.
pub(crate) fn purge_dot_hive_worker_artifacts(repo_root: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(repo_root) else {
        return 0;
    };
    let mut removed = 0u32;
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name.starts_with(".hive_worker_")
            && name.ends_with(".md")
            && std::fs::remove_file(&p).is_ok()
        {
            removed += 1;
        }
    }
    removed
}

/// Añade `.hive_worker_*.md` a `.gitignore` si aún no está, para que `git add` no los vuelva a colar.
pub(crate) fn ensure_gitignore_excludes_legacy_hive_workers(
    repo_root: &Path,
) -> std::io::Result<()> {
    const PAT: &str = ".hive_worker_*.md";
    let p = repo_root.join(".gitignore");
    let mut body = if p.exists() {
        std::fs::read_to_string(&p)?
    } else {
        String::new()
    };
    if body.lines().any(|l| l.trim() == PAT) {
        return Ok(());
    }
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }
    body.push_str("\n# The Hive — artefactos oberos antiguos (no versionar)\n");
    body.push_str(PAT);
    body.push('\n');
    std::fs::write(&p, body)
}

fn cleanup_legacy_hive_worker_md(repo_root: &Path) {
    let n = purge_dot_hive_worker_artifacts(repo_root);
    if n > 0 {
        info!(
            count = n,
            "eliminados artefactos legados .hive_worker_*.md en la raíz"
        );
    }
}

/// Un ciclo completo: análisis ADN, Consejo, cola de obreras (todas las tareas por especialista).
pub async fn run_queen_cycle(target: PathBuf, cfg: &HiveConfig) -> Result<()> {
    cfg.validate()?;
    cleanup_legacy_hive_worker_md(&target);

    let mut hive_request = crate::request::HiveRequest::resolve(&target)?;
    let client_feedback_src =
        crate::request::integrate_client_feedback(&target, &mut hive_request)?;
    if client_feedback_src != crate::request::ClientFeedbackSource::None {
        info!("La Reina integra la petición del cliente en la misión de este ciclo");
    }
    let work_mode = crate::request::effective_work_mode(&hive_request, &target);
    if hive_request.is_actionable() {
        info!(
            title = %hive_request.title,
            stack = ?hive_request.stack,
            ?work_mode,
            "solicitud del engambre cargada (bootstrap si el repo está vacío / sin manifest)"
        );
        let scaffolded = crate::request::bootstrap_if_needed(&target, &hive_request)?;
        if scaffolded && cfg.validate_scaffold {
            let stack = crate::request::stack_for_repo(&target, &hive_request);
            crate::scaffold_validate::validate_after_bootstrap(&target, stack)?;
        }
    } else {
        info!(
            ?work_mode,
            "sin hive.request accionable: ciclo orientado a revisión y mejora"
        );
    }

    ensure_gitignore_excludes_legacy_hive_workers(&target)
        .map_err(|e| anyhow::anyhow!("actualizar .gitignore: {e}"))?;

    let analyzer = RepositoryAnalyzer::new(target.clone());
    let profile = analyzer.analyze()?;

    info!(
        languages = ?profile.languages,
        frameworks = ?profile.frameworks,
        debt = ?profile.technical_debt,
        "análisis de ADN técnico completado"
    );

    let repo_path = target.clone();
    let mut orchestrator = HiveOrchestrator::new(target, profile);
    orchestrator.start(cfg, &hive_request, work_mode).await?;

    if client_feedback_src == crate::request::ClientFeedbackSource::File {
        crate::request::archive_client_feedback_file(&repo_path)?;
        info!(
            "feedback del cliente archivado bajo .hive/client_feedback_archive/ (no se repetirá en el siguiente ciclo)"
        );
    }

    info!("ciclo de La Reina finalizado");
    Ok(())
}

/// Compatibilidad: un ciclo con configuración desde entorno (`HiveConfig::from_env()`).
pub async fn run_queen_cli(target: PathBuf) -> Result<()> {
    let cfg = HiveConfig::from_env();
    run_queen_cycle(target, &cfg).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HiveConfig;
    use crate::request::{HiveRequest, ProjectStack};
    use serial_test::serial;
    use std::fs;

    #[tokio::test]
    #[serial]
    async fn run_queen_cycle_sin_solicitud_limpia_legado_y_persiste_estado() {
        let t = tempfile::tempdir().unwrap();
        std::env::remove_var("HIVE_BRANCH_MODE");
        std::env::remove_var("HIVE_REQUEST");
        std::env::remove_var("HIVE_REQUEST_JSON");
        let legacy = t
            .path()
            .join(".hive_worker_aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee.md");
        fs::write(&legacy, "legacy").unwrap();
        fs::write(
            t.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        fs::write(t.path().join("main.rs"), "fn main() {}\n").unwrap();
        crate::discovery::colonize_and_analyze(t.path()).unwrap();
        let cfg = HiveConfig {
            maintainer_reject_before_approve: 0,
            ..HiveConfig::default()
        };
        run_queen_cycle(t.path().to_path_buf(), &cfg)
            .await
            .expect("ciclo sin solicitud");
        assert!(!legacy.exists());
        assert!(t.path().join("hive.json").exists());
    }

    #[tokio::test]
    #[serial]
    async fn run_queen_cycle_archiva_hive_client_feedback_tras_exito() {
        let t = tempfile::tempdir().unwrap();
        std::env::remove_var("HIVE_BRANCH_MODE");
        std::env::remove_var("HIVE_CLIENT_FEEDBACK");
        std::env::remove_var("HIVE_REQUEST");
        std::env::remove_var("HIVE_REQUEST_JSON");
        // Crear un proyecto Rust válido
        fs::create_dir_all(t.path().join("src")).unwrap();
        fs::write(
            t.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        fs::write(t.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(
            t.path().join(crate::request::CLIENT_FEEDBACK_FILENAME),
            "Añade comentario al main.\n",
        )
        .unwrap();
        crate::discovery::colonize_and_analyze(t.path()).unwrap();
        let cfg = HiveConfig {
            maintainer_reject_before_approve: 0,
            ..HiveConfig::default()
        };
        run_queen_cycle(t.path().to_path_buf(), &cfg)
            .await
            .expect("ciclo con feedback de cliente");
        assert!(!t
            .path()
            .join(crate::request::CLIENT_FEEDBACK_FILENAME)
            .exists());
        let arch = t.path().join(".hive/client_feedback_archive");
        assert!(arch.is_dir());
        assert!(arch.read_dir().unwrap().count() >= 1);
    }

    #[tokio::test]
    #[serial]
    async fn run_queen_cycle_runs_bootstrap_from_request() {
        let t = tempfile::tempdir().unwrap();
        std::env::remove_var("HIVE_BRANCH_MODE");
        let req = HiveRequest {
            title: "Proyecto ciclo".into(),
            description: "Prueba de engambre con bootstrap.".into(),
            stack: ProjectStack::RustBinary,
            force_scaffold: false,
            ..Default::default()
        };
        fs::write(
            t.path().join(crate::request::FILENAME),
            serde_json::to_string(&req).unwrap(),
        )
        .unwrap();
        let cfg = HiveConfig {
            maintainer_reject_before_approve: 0,
            ..HiveConfig::default()
        };
        run_queen_cycle(t.path().to_path_buf(), &cfg)
            .await
            .expect("ciclo con solicitud");
        assert!(t.path().join("Cargo.toml").exists());
        assert!(t.path().join("hive.json").exists());
    }

    #[test]
    fn cleanup_legacy_worker_md_removes_files() {
        let t = tempfile::tempdir().unwrap();
        let junk = t
            .path()
            .join(".hive_worker_aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee.md");
        std::fs::write(&junk, "legacy").unwrap();
        assert_eq!(crate::purge_dot_hive_worker_artifacts(t.path()), 1);
        assert!(!junk.exists());
    }

    #[test]
    fn gitignore_gets_hive_worker_pattern() {
        let t = tempfile::tempdir().unwrap();
        crate::ensure_gitignore_excludes_legacy_hive_workers(t.path()).unwrap();
        let g = std::fs::read_to_string(t.path().join(".gitignore")).unwrap();
        assert!(g.contains(".hive_worker_*.md"));
    }

    #[test]
    fn gitignore_no_duplica_patron_hive_worker() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join(".gitignore"), ".hive_worker_*.md\n").unwrap();
        crate::ensure_gitignore_excludes_legacy_hive_workers(t.path()).unwrap();
        let g = fs::read_to_string(t.path().join(".gitignore")).unwrap();
        assert_eq!(g.matches(".hive_worker_*.md").count(), 1);
    }

    #[test]
    fn gitignore_inserta_bloque_con_salto_si_falta() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join(".gitignore"), "node_modules").unwrap();
        crate::ensure_gitignore_excludes_legacy_hive_workers(t.path()).unwrap();
        let g = fs::read_to_string(t.path().join(".gitignore")).unwrap();
        assert!(g.contains("node_modules"));
        assert!(g.contains("\n\n# The Hive"));
        assert!(g.contains(".hive_worker_*.md"));
    }

    #[test]
    fn test_purge_returns_zero_for_nonexistent_dir() {
        let result =
            crate::purge_dot_hive_worker_artifacts(std::path::Path::new("/nonexistent/path"));
        assert_eq!(result, 0);
    }
}
