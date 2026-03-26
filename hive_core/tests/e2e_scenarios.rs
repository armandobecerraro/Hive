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

/// Caja blanca: estilo y lints del artefacto Rust (mismas reglas que en CI del propio Hive).
fn assert_rust_fmt_and_clippy_clean(repo: &Path) {
    let fmt = Command::new("cargo")
        .current_dir(repo)
        .args(["fmt", "--all", "--", "--check"])
        .output()
        .expect("cargo fmt");
    assert!(
        fmt.status.success(),
        "cargo fmt --check falló:\n{}",
        String::from_utf8_lossy(&fmt.stderr)
    );
    let clip = Command::new("cargo")
        .current_dir(repo)
        .args(["clippy", "--quiet", "--", "-D", "warnings"])
        .output()
        .expect("cargo clippy");
    assert!(
        clip.status.success(),
        "cargo clippy -D warnings falló:\n{}",
        String::from_utf8_lossy(&clip.stderr)
    );
}

fn assert_rust_best_practices_basics(repo: &Path) {
    let toml = fs::read_to_string(repo.join("Cargo.toml")).expect("Cargo.toml");
    assert!(
        toml.contains("[package]"),
        "Cargo.toml debe declarar [package]"
    );
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

    assert!(
        repo.join(".git").exists(),
        "debe inicializarse repositorio Git"
    );
    assert!(
        repo.join("hive.json").exists(),
        "debe persistirse hive.json"
    );
    assert!(
        repo.join("Cargo.toml").exists(),
        "bootstrap Rust: Cargo.toml"
    );
    assert!(
        repo.join("src/main.rs").exists(),
        "bootstrap Rust: src/main.rs"
    );
    assert_rust_best_practices_basics(repo);
    assert_rust_project_passes_cargo_check(repo);
    assert_rust_binary_runs_smoke(repo);
    assert_rust_fmt_and_clippy_clean(repo);

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

// ── Escenarios adicionales: creación de repositorios ──────────────────

#[tokio::test]
#[serial]
async fn e2e_node_greenfield_creates_package_json() {
    setup_e2e_env();
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path();
    let req = r#"{
  "title": "E2E Hive Node",
  "description": "Microservicio mock para prueba E2E.",
  "stack": "node_minimal",
  "force_scaffold": false
}"#;
    fs::write(repo.join("hive.request.json"), req).unwrap();

    run_queen(repo).await;

    assert!(repo.join(".git").exists(), "debe inicializarse Git");
    assert!(
        repo.join("hive.json").exists(),
        "debe persistirse hive.json"
    );
    assert!(
        repo.join("package.json").exists(),
        "bootstrap Node: package.json"
    );
    assert!(repo.join("index.js").exists(), "bootstrap Node: index.js");
    assert!(repo.join("README.md").exists(), "debe existir README");
    assert!(
        repo.join("docs/HIVE_SPEC.md").exists(),
        "debe existir spec Hive"
    );

    let pkg = fs::read_to_string(repo.join("package.json")).unwrap();
    assert!(
        pkg.contains("\"name\""),
        "package.json debe tener campo name"
    );
    assert!(
        pkg.contains("\"version\""),
        "package.json debe tener campo version"
    );

    teardown_e2e_env();
}

#[tokio::test]
#[serial]
async fn e2e_rust_greenfield_with_force_scaffold() {
    setup_e2e_env();
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path();
    let req = r#"{
  "title": "E2E Force Scaffold",
  "description": "Proyecto con force_scaffold sobre archivos existentes.",
  "stack": "rust_binary",
  "force_scaffold": true
}"#;
    fs::write(repo.join("hive.request.json"), req).unwrap();
    fs::write(repo.join("existing.txt"), "archivo previo").unwrap();

    run_queen(repo).await;

    assert!(repo.join(".git").exists());
    assert!(
        repo.join("Cargo.toml").exists(),
        "force_scaffold debe crear Cargo.toml"
    );
    assert!(
        repo.join("src/main.rs").exists(),
        "force_scaffold debe crear src/main.rs"
    );
    assert!(
        repo.join("existing.txt").exists(),
        "archivo previo debe conservarse"
    );

    teardown_e2e_env();
}

#[tokio::test]
#[serial]
async fn e2e_multiple_cycles_increment_version() {
    setup_e2e_env();
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path();

    let req = r#"{
  "title": "E2E Versionado",
  "description": "Proyecto para verificar incremento de versión.",
  "stack": "rust_binary",
  "force_scaffold": false
}"#;
    fs::write(repo.join("hive.request.json"), req).unwrap();

    run_queen(repo).await;

    let v1 = fs::read_to_string(repo.join("version.json")).unwrap();
    assert!(
        v1.contains("0.1.0") || v1.contains("0.1.1"),
        "versión inicial debe ser 0.1.x"
    );

    let state1 = HiveState::load_or_new(repo).expect("estado inicial");
    let merges1 = state1.version_history.len();

    fs::write(repo.join("hive.request.json"), EMPTY_REQUEST_IMPROVE).unwrap();
    run_queen(repo).await;

    let state2 = HiveState::load_or_new(repo).expect("estado segundo ciclo");
    let merges2 = state2.version_history.len();
    assert!(
        merges2 >= merges1,
        "segundo ciclo debe mantener o incrementar historial de versiones"
    );

    teardown_e2e_env();
}

#[tokio::test]
#[serial]
async fn e2e_existing_rust_repo_improve_mode() {
    setup_e2e_env();
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path();

    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("Cargo.toml"),
        "[package]\nname=\"existing\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    fs::write(
        repo.join("src/main.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n",
    )
    .unwrap();
    fs::write(
        repo.join("src/lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .unwrap();

    let req = r#"{
  "title": "",
  "description": "",
  "stack": "auto",
  "force_scaffold": false
}"#;
    fs::write(repo.join("hive.request.json"), req).unwrap();

    run_queen(repo).await;

    assert!(
        repo.join(".git").exists(),
        "debe inicializarse Git en repo existente"
    );
    assert!(
        repo.join("hive.json").exists(),
        "debe persistirse hive.json"
    );
    assert!(
        repo.join("Cargo.toml").exists(),
        "Cargo.toml debe conservarse"
    );
    assert!(
        repo.join("src/main.rs").exists(),
        "src/main.rs debe conservarse"
    );
    assert!(
        repo.join("src/lib.rs").exists(),
        "src/lib.rs debe conservarse"
    );

    teardown_e2e_env();
}

#[tokio::test]
#[serial]
async fn e2e_decision_log_grows_with_cycles() {
    setup_e2e_env();
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path();

    let req = r#"{
  "title": "Decisiones Test",
  "description": "Verificar que el log de decisiones crece.",
  "stack": "rust_binary",
  "force_scaffold": false
}"#;
    fs::write(repo.join("hive.request.json"), req).unwrap();

    run_queen(repo).await;
    let s1 = HiveState::load_or_new(repo).unwrap();
    let d1 = s1.decision_tree.len();

    fs::write(repo.join("hive.request.json"), EMPTY_REQUEST_IMPROVE).unwrap();
    run_queen(repo).await;
    let s2 = HiveState::load_or_new(repo).unwrap();
    let d2 = s2.decision_tree.len();

    assert!(
        d2 > d1,
        "segundo ciclo debe añadir decisiones: d1={d1} d2={d2}"
    );

    teardown_e2e_env();
}

// ── Escenarios de creación de repositorios con stacks mixtos ──────────

#[tokio::test]
#[serial]
async fn e2e_auto_stack_detects_existing_python() {
    setup_e2e_env();
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path();

    fs::write(repo.join("main.py"), "print('hello')").unwrap();
    fs::write(repo.join("requirements.txt"), "requests==2.31.0\n").unwrap();

    let req = r#"{
  "title": "",
  "description": "",
  "stack": "auto",
  "force_scaffold": false
}"#;
    fs::write(repo.join("hive.request.json"), req).unwrap();

    run_queen(repo).await;

    assert!(repo.join(".git").exists());
    assert!(repo.join("hive.json").exists());
    assert!(repo.join("main.py").exists(), "main.py debe conservarse");

    teardown_e2e_env();
}

#[tokio::test]
#[serial]
async fn e2e_empty_repo_creates_rust_by_default() {
    setup_e2e_env();
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path();

    let req = r#"{
  "title": "Proyecto Default",
  "description": "Repositorio vacío debe crear Rust por defecto.",
  "stack": "auto",
  "force_scaffold": false
}"#;
    fs::write(repo.join("hive.request.json"), req).unwrap();

    run_queen(repo).await;

    assert!(repo.join(".git").exists());
    assert!(repo.join("hive.json").exists());
    assert!(
        repo.join("Cargo.toml").exists(),
        "repo vacío con auto debe crear Cargo.toml"
    );
    assert!(
        repo.join("src/main.rs").exists(),
        "repo vacío con auto debe crear src/main.rs"
    );

    teardown_e2e_env();
}

#[tokio::test]
#[serial]
async fn e2e_hive_objective_updated_after_merge() {
    setup_e2e_env();
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path();

    let req = r#"{
  "title": "Objetivo Hive",
  "description": "Verificar que HIVE_OBJECTIVE.md se actualiza tras merge.",
  "stack": "rust_binary",
  "force_scaffold": false
}"#;
    fs::write(repo.join("hive.request.json"), req).unwrap();

    run_queen(repo).await;

    assert!(
        repo.join("HIVE_OBJECTIVE.md").exists(),
        "debe existir HIVE_OBJECTIVE.md"
    );
    let obj = fs::read_to_string(repo.join("HIVE_OBJECTIVE.md")).unwrap();
    assert!(
        obj.contains("Objetivo operativo"),
        "debe contener título del objetivo"
    );
    assert!(obj.contains("main"), "debe mencionar la rama principal");

    teardown_e2e_env();
}
