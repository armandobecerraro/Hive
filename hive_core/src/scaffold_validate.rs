//! Tras el andamiaje greenfield, exige el mismo nivel de comprobaciones que un buen CLI:
//! `cargo check` / `fmt` / `clippy` / `test` (Rust), `py_compile` (Python), `node --check` (Node).

use crate::request::ProjectStack;
use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// Ejecuta la batería de comprobaciones según el stack detectado.
pub fn validate_after_bootstrap(repo: &Path, stack: ProjectStack) -> Result<()> {
    match stack {
        ProjectStack::RustBinary | ProjectStack::Auto => validate_rust(repo),
        ProjectStack::PythonApp => validate_python(repo),
        ProjectStack::NodeMinimal => validate_node(repo),
        ProjectStack::FlutterApp => validate_flutter(repo),
    }
}

fn validate_flutter(repo: &Path) -> Result<()> {
    if !repo.join("pubspec.yaml").exists() {
        return Ok(());
    }
    let flutter_ok = Command::new("flutter")
        .current_dir(repo)
        .args(["--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !flutter_ok {
        tracing::warn!(
            "Flutter SDK no encontrado en PATH: omite `flutter analyze` (instala Flutter o añade al PATH)"
        );
        return Ok(());
    }
    run_cmd(
        repo,
        "flutter",
        &["pub", "get"],
        "flutter pub get (¿Flutter en PATH?)",
    )?;
    run_cmd(
        repo,
        "flutter",
        &["analyze", "--no-fatal-infos"],
        "flutter analyze",
    )?;
    Ok(())
}

fn validate_rust(repo: &Path) -> Result<()> {
    if !repo.join("Cargo.toml").exists() {
        return Ok(());
    }
    run_cmd(
        repo,
        "cargo",
        &["check", "-q"],
        "cargo check (¿rustup/cargo en PATH?)",
    )?;
    run_cmd(
        repo,
        "cargo",
        &["fmt", "--all", "--", "--check"],
        "cargo fmt --check (¿rustfmt instalado? rustup component add rustfmt)",
    )?;
    run_cmd(
        repo,
        "cargo",
        &["clippy", "--all-targets", "--", "-D", "warnings"],
        "cargo clippy -D warnings (¿clippy? rustup component add clippy)",
    )?;
    run_cmd(
        repo,
        "cargo",
        &["test", "--no-fail-fast", "-q"],
        "cargo test",
    )?;
    Ok(())
}

fn validate_python(repo: &Path) -> Result<()> {
    let main_py = repo.join("main.py");
    if main_py.exists() {
        run_cmd(
            repo,
            "python3",
            &["-m", "py_compile", "main.py"],
            "python3 -m py_compile main.py",
        )?;
    }
    Ok(())
}

fn validate_node(repo: &Path) -> Result<()> {
    let index = repo.join("index.js");
    if index.exists() {
        run_cmd(
            repo,
            "node",
            &["--check", "index.js"],
            "node --check index.js (¿Node en PATH?)",
        )?;
    }
    Ok(())
}

fn run_cmd(repo: &Path, program: &str, args: &[&str], label: &str) -> Result<()> {
    let out = Command::new(program)
        .current_dir(repo)
        .args(args)
        .output()
        .with_context(|| format!("ejecutar {label} (¿`{program}` en PATH?)"))?;
    if out.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    anyhow::bail!("{label} falló:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_python_sin_main_py_retorna_ok() {
        let tmp = tempfile::tempdir().unwrap();
        // No hay main.py, debe retornar Ok
        let result = validate_python(tmp.path());
        assert!(result.is_ok());
    }

    #[test]
    fn validate_node_sin_index_js_retorna_ok() {
        let tmp = tempfile::tempdir().unwrap();
        // No hay index.js, debe retornar Ok
        let result = validate_node(tmp.path());
        assert!(result.is_ok());
    }

    #[test]
    fn validate_rust_sin_cargo_toml_retorna_ok() {
        let tmp = tempfile::tempdir().unwrap();
        // No hay Cargo.toml, debe retornar Ok
        let result = validate_rust(tmp.path());
        assert!(result.is_ok());
    }

    #[test]
    fn validate_after_bootstrap_python_sin_archivos() {
        let tmp = tempfile::tempdir().unwrap();
        let result = validate_after_bootstrap(tmp.path(), ProjectStack::PythonApp);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_after_bootstrap_node_sin_archivos() {
        let tmp = tempfile::tempdir().unwrap();
        let result = validate_after_bootstrap(tmp.path(), ProjectStack::NodeMinimal);
        assert!(result.is_ok());
    }
}
