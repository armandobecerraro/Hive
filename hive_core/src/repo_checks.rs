//! Comprobaciones de calidad por tipo de repo antes del MR y para el Consejo (Rust / Flutter-Dart).

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Si hay `Cargo.toml`, ejecuta `cargo test`. Si hay `pubspec.yaml` (Flutter/Dart), `flutter pub get`,
/// `flutter analyze` y `flutter test` (o equivalente `dart` si no hay SDK Flutter en el pubspec).
pub fn preflight_tests_before_mr(repo_root: &Path) -> Result<()> {
    if repo_root.join("Cargo.toml").exists() {
        return preflight_cargo_test(repo_root);
    }
    if repo_root.join("pubspec.yaml").exists() {
        return preflight_pubspec_project(repo_root);
    }
    Ok(())
}

/// Igual que [`preflight_tests_before_mr`] pero solo expuesto para tests.
pub fn preflight_cargo_test(repo_root: &Path) -> Result<()> {
    let out = Command::new("cargo")
        .current_dir(repo_root)
        .args(["test", "--no-fail-fast", "-q"])
        .output()
        .context("ejecutar cargo test (¿cargo en PATH?)")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        anyhow::bail!("cargo test falló antes del MR:\n{stdout}\n{stderr}");
    }
    Ok(())
}

fn preflight_pubspec_project(repo_root: &Path) -> Result<()> {
    if uses_flutter_sdk(repo_root) {
        if !command_ok("flutter", &["--version"]) {
            anyhow::bail!(
                "proyecto Flutter (`pubspec.yaml` con SDK Flutter) pero `flutter` no está en PATH; instala Flutter o añade al PATH para ejecutar pub get / analyze / test antes del MR"
            );
        }
        run_cmd(
            repo_root,
            "flutter",
            &["pub", "get"],
            "flutter pub get",
        )?;
        run_cmd(
            repo_root,
            "flutter",
            &["analyze", "--no-fatal-infos"],
            "flutter analyze",
        )?;
        run_cmd(
            repo_root,
            "flutter",
            &["test", "--no-pub", "-r", "compact"],
            "flutter test",
        )?;
        return Ok(());
    }

    // Paquete Dart puro (sin dependencia `flutter` del SDK)
    if !command_ok("dart", &["--version"]) {
        anyhow::bail!(
            "pubspec.yaml sin SDK Flutter pero `dart` no está en PATH (instala Dart o usa un proyecto Flutter)"
        );
    }
    run_cmd(repo_root, "dart", &["pub", "get"], "dart pub get")?;
    run_cmd(repo_root, "dart", &["analyze"], "dart analyze")?;
    run_cmd(repo_root, "dart", &["test"], "dart test")?;
    Ok(())
}

fn uses_flutter_sdk(repo_root: &Path) -> bool {
    let Ok(raw) = fs::read_to_string(repo_root.join("pubspec.yaml")) else {
        return false;
    };
    raw.contains("flutter:")
        && (raw.contains("sdk: flutter") || raw.contains("sdk:flutter") || raw.contains("sdk: \"flutter\""))
}

fn command_ok(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_cmd(repo_root: &Path, program: &str, args: &[&str], label: &str) -> Result<()> {
    let out = Command::new(program)
        .current_dir(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("ejecutar {label} (¿`{program}` en PATH?)"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        anyhow::bail!("{label} falló antes del MR:\n{stdout}\n{stderr}");
    }
    Ok(())
}

/// Para el Consejo: ¿los tests del repo pasan? (Rust o Flutter/Dart)
pub fn council_tests_passed(repo_root: &Path) -> bool {
    if repo_root.join("Cargo.toml").exists() {
        return Command::new("cargo")
            .current_dir(repo_root)
            .args(["test", "--quiet", "--no-fail-fast"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(true);
    }
    if repo_root.join("pubspec.yaml").exists() {
        if uses_flutter_sdk(repo_root) {
            if !command_ok("flutter", &["--version"]) {
                return true;
            }
            return Command::new("flutter")
                .current_dir(repo_root)
                .args(["test", "--no-pub", "-r", "compact"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(true);
        }
        if command_ok("dart", &["--version"]) {
            return Command::new("dart")
                .current_dir(repo_root)
                .args(["test"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(true);
        }
        return true;
    }
    true
}

/// Ejecuta tests para [`crate::validator::ResultValidator::validate_full`] (Rust o Dart/Flutter).
pub fn validator_run_tests(repo_root: &Path) -> Result<(bool, String), String> {
    if repo_root.join("Cargo.toml").exists() {
        let output = Command::new("cargo")
            .current_dir(repo_root)
            .args(["test", "--no-fail-fast", "-q"])
            .output()
            .map_err(|e| e.to_string())?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Ok((output.status.success(), format!("{stdout}\n{stderr}")));
    }
    if repo_root.join("pubspec.yaml").exists() {
        if uses_flutter_sdk(repo_root) {
            if !command_ok("flutter", &["--version"]) {
                return Ok((true, "Flutter no está en PATH; tests omitidos.".into()));
            }
            let output = Command::new("flutter")
                .current_dir(repo_root)
                .args(["test", "--no-pub", "-r", "compact"])
                .output()
                .map_err(|e| e.to_string())?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Ok((output.status.success(), format!("{stdout}\n{stderr}")));
        }
        if !command_ok("dart", &["--version"]) {
            return Ok((true, "Dart no está en PATH; tests omitidos.".into()));
        }
        let output = Command::new("dart")
            .current_dir(repo_root)
            .args(["test"])
            .output()
            .map_err(|e| e.to_string())?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Ok((output.status.success(), format!("{stdout}\n{stderr}")));
    }
    Ok((true, "Sin Cargo.toml ni pubspec.yaml; tests omitidos.".into()))
}

/// Comprobación de compilación/análisis estático para el validador (`cargo check` o analizador Dart).
pub fn validator_check_compilation(repo_root: &Path) -> Result<(bool, String), String> {
    if repo_root.join("Cargo.toml").exists() {
        let output = Command::new("cargo")
            .current_dir(repo_root)
            .args(["check", "-q"])
            .output()
            .map_err(|e| e.to_string())?;
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Ok((output.status.success(), stderr));
    }
    if repo_root.join("pubspec.yaml").exists() {
        if uses_flutter_sdk(repo_root) {
            if !command_ok("flutter", &["--version"]) {
                return Ok((true, "Flutter no está en PATH; analyze omitido.".into()));
            }
            let output = Command::new("flutter")
                .current_dir(repo_root)
                .args(["analyze", "--no-fatal-infos"])
                .output()
                .map_err(|e| e.to_string())?;
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            return Ok((output.status.success(), format!("{stdout}\n{stderr}")));
        }
        if !command_ok("dart", &["--version"]) {
            return Ok((true, "Dart no está en PATH; analyze omitido.".into()));
        }
        let output = Command::new("dart")
            .current_dir(repo_root)
            .args(["analyze"])
            .output()
            .map_err(|e| e.to_string())?;
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        return Ok((output.status.success(), format!("{stdout}\n{stderr}")));
    }
    Ok((true, "Sin proyecto compilable conocido.".into()))
}

/// Contador de avisos para el score: clippy (Rust) o salida de `dart analyze` / `flutter analyze`.
pub fn council_warning_count(repo_root: &Path) -> u32 {
    if repo_root.join("Cargo.toml").exists() {
        return count_clippy_warnings(repo_root);
    }
    if repo_root.join("pubspec.yaml").exists() {
        return count_analyzer_issues(repo_root);
    }
    0
}

fn count_clippy_warnings(repo_root: &Path) -> u32 {
    let output = Command::new("cargo")
        .current_dir(repo_root)
        .args(["clippy", "--quiet", "--", "-W", "clippy::all"])
        .stderr(std::process::Stdio::piped())
        .output();

    match output {
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            stderr.lines().filter(|l| l.contains("warning[")).count() as u32
        }
        Err(_) => 0,
    }
}

fn count_analyzer_issues(repo_root: &Path) -> u32 {
    let (program, args): (&str, &[&str]) = if uses_flutter_sdk(repo_root) {
        if !command_ok("flutter", &["--version"]) {
            return 0;
        }
        ("flutter", &["analyze", "--no-fatal-infos"])
    } else if command_ok("dart", &["--version"]) {
        ("dart", &["analyze"])
    } else {
        return 0;
    };

    let output = Command::new(program)
        .current_dir(repo_root)
        .args(args)
        .output();

    match output {
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let combined = format!("{stdout}\n{stderr}");
            combined
                .lines()
                .filter(|l| l.contains("error •") || l.contains("warning •"))
                .count() as u32
        }
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_flutter_sdk_detects_sample() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("pubspec.yaml"),
            "dependencies:\n  flutter:\n    sdk: flutter\n",
        )
        .unwrap();
        assert!(uses_flutter_sdk(tmp.path()));
    }

    #[test]
    fn uses_flutter_sdk_false_for_pure_dart() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("pubspec.yaml"),
            "name: x\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\n",
        )
        .unwrap();
        assert!(!uses_flutter_sdk(tmp.path()));
    }
}
