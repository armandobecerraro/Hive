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
pub mod sequential_merger;

// Módulos multi-agente (opcionales, requieren feature "multiagent")
#[cfg(feature = "multiagent")]
pub mod blackboard;
#[cfg(feature = "multiagent")]
pub mod brain;
#[cfg(feature = "multiagent")]
pub mod worker;

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
        crate::request::bootstrap_if_needed(&target, &hive_request)?;
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
