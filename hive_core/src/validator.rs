//! Validador de resultados reales.
//!
//! Verifica que los cambios de una obrera realmente resuelven la tarea:
//! aplica patches, ejecuta tests, y confirma que el issue/request fue resuelto.
//! Inspirado en el harness de evaluación SWE-bench.

use serde::{Deserialize, Serialize};

/// Resultado de validación.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub passed: bool,
    pub patch_applied: bool,
    pub tests_passed: bool,
    pub compilation_ok: bool,
    pub issues_resolved: Vec<String>,
    pub issues_remaining: Vec<String>,
    pub error: Option<String>,
    pub details: String,
}

/// Configuración del validador.
#[derive(Debug, Clone)]
pub struct ValidatorConfig {
    pub run_tests: bool,
    pub check_compilation: bool,
    pub timeout_secs: u64,
}

impl Default for ValidatorConfig {
    fn default() -> Self {
        Self {
            run_tests: true,
            check_compilation: true,
            timeout_secs: 120,
        }
    }
}

/// Validador de resultados.
pub struct ResultValidator;

impl ResultValidator {
    /// Valida que un patch se aplica correctamente.
    pub fn validate_patch_apply(repo_root: &std::path::Path, patch: &str) -> Result<bool, String> {
        // Escribir patch a archivo temporal
        let patch_file = repo_root.join(".hive").join("temp.patch");
        if let Some(parent) = patch_file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&patch_file, patch).map_err(|e| e.to_string())?;

        // Intentar aplicar con git apply --check
        let output = std::process::Command::new("git")
            .args(["apply", "--check"])
            .arg(&patch_file)
            .current_dir(repo_root)
            .output()
            .map_err(|e| e.to_string())?;

        let _ = std::fs::remove_file(&patch_file);
        Ok(output.status.success())
    }

    /// Ejecuta tests y retorna resultado.
    pub fn run_tests(repo_root: &std::path::Path) -> Result<(bool, String), String> {
        crate::repo_checks::validator_run_tests(repo_root)
    }

    /// Verifica compilación.
    pub fn check_compilation(repo_root: &std::path::Path) -> Result<(bool, String), String> {
        crate::repo_checks::validator_check_compilation(repo_root)
    }

    /// Validación completa: patch + compilación + tests.
    pub fn validate_full(
        repo_root: &std::path::Path,
        patch: Option<&str>,
        config: &ValidatorConfig,
    ) -> ValidationResult {
        let mut result = ValidationResult {
            passed: true,
            patch_applied: true,
            tests_passed: true,
            compilation_ok: true,
            issues_resolved: vec![],
            issues_remaining: vec![],
            error: None,
            details: String::new(),
        };

        // Validar patch
        if let Some(patch_content) = patch {
            match Self::validate_patch_apply(repo_root, patch_content) {
                Ok(ok) => {
                    result.patch_applied = ok;
                    if !ok {
                        result.passed = false;
                        result
                            .details
                            .push_str("Patch no se aplica correctamente.\n");
                    }
                }
                Err(e) => {
                    result.patch_applied = false;
                    result.passed = false;
                    result.error = Some(e);
                }
            }
        }

        // Verificar compilación
        if config.check_compilation {
            match Self::check_compilation(repo_root) {
                Ok((ok, output)) => {
                    result.compilation_ok = ok;
                    if !ok {
                        result.passed = false;
                        result
                            .details
                            .push_str(&format!("Compilación falló: {output}\n"));
                    }
                }
                Err(e) => {
                    result.compilation_ok = false;
                    result.passed = false;
                    result
                        .details
                        .push_str(&format!("Error de compilación: {e}\n"));
                }
            }
        }

        // Ejecutar tests
        if config.run_tests {
            match Self::run_tests(repo_root) {
                Ok((ok, output)) => {
                    result.tests_passed = ok;
                    if !ok {
                        result.passed = false;
                        result
                            .details
                            .push_str(&format!("Tests fallaron: {output}\n"));
                    }
                }
                Err(e) => {
                    result.tests_passed = false;
                    result.passed = false;
                    result
                        .details
                        .push_str(&format!("Error ejecutando tests: {e}\n"));
                }
            }
        }

        if result.passed {
            result.details = "Todos los checks pasaron.".into();
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validator_config_default() {
        let cfg = ValidatorConfig::default();
        assert!(cfg.run_tests);
        assert!(cfg.check_compilation);
    }

    #[test]
    fn validation_result_defaults() {
        let r = ValidationResult {
            passed: true,
            patch_applied: true,
            tests_passed: true,
            compilation_ok: true,
            issues_resolved: vec![],
            issues_remaining: vec![],
            error: None,
            details: String::new(),
        };
        assert!(r.passed);
    }
}
