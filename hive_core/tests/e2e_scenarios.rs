//! Escenarios de prueba **end-to-end** de la colmena: repositorio desde cero, ciclo de mejora,
//! iteración con feedback del cliente y comprobaciones de “calidad mínima” (compilación + buenas prácticas básicas).
//!
//! Requisitos: `cargo` en `PATH` para validar proyectos Rust generados. Variables de entorno
//! se fijan por prueba y se limpian al terminar (`HIVE_SKIP_RESOURCE_GATE` evita bloqueos en CI).

use hive_core::run_queen_cli;
use hive_core::state::HiveState;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

use serial_test::serial;

const GREENFIELD_RUST_REQUEST: &str = r#"{
  "title": "E2E Hive Greenfield",
  "description": "Proyecto mínimo generado por prueba end-to-end.",
  "stack": "rust_binary",
  "force_scaffold": false
}"#;

const EMPTY_REQUEST_IMPROVE: &str = r#"{
  "title": "",
  "description": "",
  "stack": "auto",
  "force_scaffold": false
}"#;

fn setup_e2e_env() {
    std::env::set_var("HIVE_SKIP_RESOURCE_GATE", "1");
    // Acelera el Consejo: sin rechazos simulados antes del primer approve.
    std::env::set_var("HIVE_MAINTAINER_REJECT_BEFORE_APPROVE", "0");
    std::env::remove_var("HIVE_BRANCH_MODE");
    std::env::remove_var("HIVE_USE_MOCK_LLM");
    std::env::remove_var("HIVE_REQUEST_JSON");
    std::env::remove_var("HIVE_REQUEST");
}

fn teardown_e2e_env() {
    std::env::remove_var("HIVE_SKIP_RESOURCE_GATE");
    std::env::remove_var("HIVE_MAINTAINER_REJECT_BEFORE_APPROVE");
}

fn write_greenfield_request(repo: &Path) {
    fs::write(repo.join("hive.request.json"), GREENFIELD_RUST_REQUEST).unwrap();
}

async fn run_queen(repo: &Path) {
    run_queen_cli(repo.to_path_buf())
        .await
        .expect("run_queen_cli debe completar sin error");
}

fn assert_rust_project_passes_cargo_check(repo: &Path) {
    let out = Command::new("cargo")
        .current_dir(repo)
        .args(["check", "--quiet"])
        .output()
        .expect("ejecutar cargo check (¿cargo en PATH?)");
    assert!(
        out.status.success(),
        "cargo check falló:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Caja negra mínima: el binario debe ejecutar sin panic (salida 0).
fn assert_rust_binary_runs_smoke(repo: &Path) {
    let out = Command::new("cargo")
        .current_dir(repo)
        .args(["run", "--quiet"])
        .output()
        .expect("cargo run");
    assert!(
        out.status.success(),
        "cargo run (smoke) falló:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn assert_rust_best_practices_basics(repo: &Path) {
    let toml = fs::read_to_string(repo.join("Cargo.toml")).expect("Cargo.toml");
    assert!(toml.contains("[package]"), "Cargo.toml debe declarar [package]");
    assert!(
        toml.contains("edition"),
        "Cargo.toml debe fijar edition (práctica recomendada)"
    );
    let gitignore = fs::read_to_string(repo.join(".gitignore")).expect(".gitignore");
    assert!(
        gitignore.contains("target"),
        ".gitignore debe ignorar artefactos de build (/target)"
    );
    assert!(
        repo.join("README.md").exists(),
        "debe existir README para documentar el proyecto"
    );
}

#[tokio::test]
#[serial]
async fn e2e_greenfield_rust_quality_gates() {
    setup_e2e_env();
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path();
    write_greenfield_request(repo);

    run_queen(repo).await;

    assert!(repo.join(".git").exists(), "debe inicializarse repositorio Git");
    assert!(repo.join("hive.json").exists(), "debe persistirse hive.json");
    assert!(repo.join("Cargo.toml").exists(), "bootstrap Rust: Cargo.toml");
    assert!(repo.join("src/main.rs").exists(), "bootstrap Rust: src/main.rs");
    assert_rust_best_practices_basics(repo);
    assert_rust_project_passes_cargo_check(repo);
    assert_rust_binary_runs_smoke(repo);

    teardown_e2e_env();
}

#[tokio::test]
#[serial]
async fn e2e_second_cycle_improve_adds_decisions() {
    setup_e2e_env();
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path();
    write_greenfield_request(repo);
    run_queen(repo).await;

    let after_first = HiveState::load_or_new(repo).expect("hive.json");
    let n1 = after_first.decision_tree.len();

    fs::write(repo.join("hive.request.json"), EMPTY_REQUEST_IMPROVE).unwrap();
    run_queen(repo).await;

    let after_second = HiveState::load_or_new(repo).expect("hive.json");
    let n2 = after_second.decision_tree.len();
    assert!(
        n2 > n1 || !after_second.version_history.is_empty(),
        "segundo ciclo (mejora) debe añadir decisiones o historial de versiones: n1={n1} n2={n2}"
    );

    teardown_e2e_env();
}

#[tokio::test]
#[serial]
async fn e2e_client_feedback_archived_after_cycle() {
    setup_e2e_env();
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path();
    write_greenfield_request(repo);
    run_queen(repo).await;

    fs::write(
        repo.join("hive.client_feedback.md"),
        "Añadir nota en README indicando validación E2E.",
    )
    .unwrap();

    run_queen(repo).await;

    let archive = repo.join(".hive").join("client_feedback_archive");
    let count = fs::read_dir(&archive)
        .expect("directorio de archivo de feedback")
        .count();
    assert!(
        count >= 1,
        "hive.client_feedback.md debe archivarse bajo .hive/client_feedback_archive/"
    );

    teardown_e2e_env();
}

#[tokio::test]
#[serial]
async fn e2e_python_greenfield_compiles_minimal() {
    setup_e2e_env();
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path();
    let req = r#"{
  "title": "E2E Hive Python",
  "description": "App mínima para prueba E2E.",
  "stack": "python_app",
  "force_scaffold": false
}"#;
    fs::write(repo.join("hive.request.json"), req).unwrap();

    run_queen(repo).await;

    assert!(repo.join(".git").exists());
    assert!(repo.join("hive.json").exists());
    assert!(
        repo.join("pyproject.toml").exists() || repo.join("main.py").exists(),
        "bootstrap Python debe crear pyproject.toml o main.py"
    );
    let main_py = repo.join("main.py");
    if main_py.exists() {
        let out = Command::new("python3")
            .current_dir(repo)
            .args(["-m", "py_compile", "main.py"])
            .output()
            .expect("python3 -m py_compile (¿Python instalado?)");
        assert!(
            out.status.success(),
            "main.py debe compilar sintácticamente:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    teardown_e2e_env();
}
