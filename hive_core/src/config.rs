//! Configuración por variables de entorno (documentadas en README o comentarios).

use anyhow::Result;
use std::time::Duration;

/// Ajustes de runtime para La Reina, el Consejo y retención de `hive.json`.
#[derive(Debug, Clone)]
pub struct HiveConfig {
    /// Pausa entre ciclos completos en modo daemon.
    pub poll_interval: Duration,
    /// Máximo de entradas en `decision_tree` (las más antiguas se descartan).
    pub max_decision_records: usize,
    /// Máximo de entradas en `version_history`.
    pub max_version_history_entries: usize,
    /// Rechazos simulados del Mantenedor antes del primer approve (por MR).
    pub maintainer_reject_before_approve: u32,
    /// Tras tantos rechazos del Consejo sobre la misma obrera, se **descarta** el trabajo (borra rama, vuelve a la línea de integración) y la cola **sigue** con otras tareas (evita bucles infinitos).
    pub max_mr_rejection_attempts: u32,
    /// Si es true, antes de enviar el MR al Consejo ejecuta `cargo test` en repos con `Cargo.toml` (fallo → no se abre MR y se reintenta como rechazo lógico vía error en la obrera).
    pub run_tests_before_mr: bool,
    /// Si es true, el enjambre no escribe ni fusiona en `main`; los merges y commits de hitos van a [`Self::integration_branch`].
    pub protect_main: bool,
    /// Rama donde convergen merges aprobados y `HIVE_OBJECTIVE.md` (por defecto `main` si `protect_main` es false, o `hive/integration` si es true y no se define env).
    pub integration_branch: String,
}

impl Default for HiveConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(10),
            max_decision_records: 2000,
            max_version_history_entries: 500,
            maintainer_reject_before_approve: 1,
            max_mr_rejection_attempts: 10,
            run_tests_before_mr: false,
            protect_main: false,
            integration_branch: "main".to_string(),
        }
    }
}

impl HiveConfig {
    /// Comprueba coherencia de política (p. ej. no usar `main` como rama de integración si `main` está protegida).
    pub fn validate(&self) -> Result<()> {
        if self.protect_main {
            let ib = self.integration_branch.trim();
            if ib.is_empty() {
                anyhow::bail!("HIVE_INTEGRATION_BRANCH vacío con HIVE_PROTECT_MAIN activo");
            }
            if ib == "main" || ib == "master" {
                anyhow::bail!(
                    "con HIVE_PROTECT_MAIN=1, HIVE_INTEGRATION_BRANCH no puede ser `main` ni `master`; use p. ej. `hive/integration`"
                );
            }
        }
        Ok(())
    }

    /// Lee variables de entorno; valores inválidos ignorados → default del campo.
    pub fn from_env() -> Self {
        let poll_secs = std::env::var("HIVE_POLL_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(10);

        let max_decision_records = std::env::var("HIVE_MAX_DECISION_RECORDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&n| n > 0)
            .unwrap_or(2000);

        let max_version_history_entries = std::env::var("HIVE_MAX_VERSION_HISTORY")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&n| n > 0)
            .unwrap_or(500);

        let maintainer_reject_before_approve =
            std::env::var("HIVE_MAINTAINER_REJECT_BEFORE_APPROVE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);

        let max_mr_rejection_attempts = std::env::var("HIVE_MAX_MR_REJECTION_ATTEMPTS")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&n| n > 0)
            .unwrap_or(10);

        let run_tests_before_mr = matches!(
            std::env::var("HIVE_RUN_TESTS_BEFORE_MR").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
        );

        let protect_main = matches!(
            std::env::var("HIVE_PROTECT_MAIN").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
        );

        let integration_branch = match std::env::var("HIVE_INTEGRATION_BRANCH") {
            Ok(s) => {
                let t = s.trim();
                if t.is_empty() {
                    if protect_main {
                        "hive/integration".to_string()
                    } else {
                        "main".to_string()
                    }
                } else {
                    t.to_string()
                }
            }
            Err(_) => {
                if protect_main {
                    "hive/integration".to_string()
                } else {
                    "main".to_string()
                }
            }
        };

        Self {
            poll_interval: Duration::from_secs(poll_secs),
            max_decision_records,
            max_version_history_entries,
            maintainer_reject_before_approve,
            max_mr_rejection_attempts,
            run_tests_before_mr,
            protect_main,
            integration_branch,
        }
    }

    /// `HIVE_DAEMON=1` o `true` activa bucle en el binario (también existe `--daemon`).
    pub fn daemon_from_env() -> bool {
        matches!(
            std::env::var("HIVE_DAEMON").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn default_values() {
        let c = HiveConfig::default();
        assert_eq!(c.poll_interval.as_secs(), 10);
        assert_eq!(c.max_decision_records, 2000);
        assert_eq!(c.maintainer_reject_before_approve, 1);
        assert_eq!(c.max_mr_rejection_attempts, 10);
        assert!(!c.run_tests_before_mr);
        assert!(!c.protect_main);
        assert_eq!(c.integration_branch, "main");
        assert!(c.validate().is_ok());
    }

    #[test]
    #[serial]
    fn from_env_respects_poll() {
        std::env::set_var("HIVE_POLL_INTERVAL_SECS", "42");
        let c = HiveConfig::from_env();
        std::env::remove_var("HIVE_POLL_INTERVAL_SECS");
        assert_eq!(c.poll_interval.as_secs(), 42);
    }

    #[test]
    #[serial]
    fn from_env_invalid_poll_uses_default() {
        std::env::set_var("HIVE_POLL_INTERVAL_SECS", "0");
        let c = HiveConfig::from_env();
        std::env::remove_var("HIVE_POLL_INTERVAL_SECS");
        assert_eq!(c.poll_interval.as_secs(), 10);
    }

    #[test]
    #[serial]
    fn daemon_from_env_true() {
        std::env::set_var("HIVE_DAEMON", "1");
        assert!(HiveConfig::daemon_from_env());
        std::env::remove_var("HIVE_DAEMON");
    }

    #[test]
    #[serial]
    fn daemon_from_env_false_when_unset() {
        std::env::remove_var("HIVE_DAEMON");
        assert!(!HiveConfig::daemon_from_env());
    }

    #[test]
    #[serial]
    fn from_env_max_records_and_version_cap() {
        std::env::set_var("HIVE_MAX_DECISION_RECORDS", "100");
        std::env::set_var("HIVE_MAX_VERSION_HISTORY", "50");
        let c = HiveConfig::from_env();
        std::env::remove_var("HIVE_MAX_DECISION_RECORDS");
        std::env::remove_var("HIVE_MAX_VERSION_HISTORY");
        assert_eq!(c.max_decision_records, 100);
        assert_eq!(c.max_version_history_entries, 50);
    }

    #[test]
    #[serial]
    fn from_env_invalid_max_falls_back() {
        std::env::set_var("HIVE_MAX_DECISION_RECORDS", "0");
        let c = HiveConfig::from_env();
        std::env::remove_var("HIVE_MAX_DECISION_RECORDS");
        assert_eq!(c.max_decision_records, 2000);
    }

    #[test]
    #[serial]
    fn from_env_max_mr_rejection_attempts() {
        std::env::set_var("HIVE_MAX_MR_REJECTION_ATTEMPTS", "7");
        let c = HiveConfig::from_env();
        std::env::remove_var("HIVE_MAX_MR_REJECTION_ATTEMPTS");
        assert_eq!(c.max_mr_rejection_attempts, 7);
    }

    #[test]
    #[serial]
    fn from_env_maintainer_rejects() {
        std::env::set_var("HIVE_MAINTAINER_REJECT_BEFORE_APPROVE", "2");
        let c = HiveConfig::from_env();
        std::env::remove_var("HIVE_MAINTAINER_REJECT_BEFORE_APPROVE");
        assert_eq!(c.maintainer_reject_before_approve, 2);
    }

    #[test]
    #[serial]
    fn from_env_run_tests_before_mr_true() {
        std::env::set_var("HIVE_RUN_TESTS_BEFORE_MR", "yes");
        let c = HiveConfig::from_env();
        std::env::remove_var("HIVE_RUN_TESTS_BEFORE_MR");
        assert!(c.run_tests_before_mr);
    }

    #[test]
    #[serial]
    fn daemon_from_env_true_yes_variant() {
        std::env::set_var("HIVE_DAEMON", "YES");
        assert!(HiveConfig::daemon_from_env());
        std::env::remove_var("HIVE_DAEMON");
    }

    #[test]
    #[serial]
    fn daemon_from_env_true_lowercase_true() {
        std::env::set_var("HIVE_DAEMON", "true");
        assert!(HiveConfig::daemon_from_env());
        std::env::remove_var("HIVE_DAEMON");
    }

    #[test]
    #[serial]
    fn from_env_run_tests_before_mr_numeric_one() {
        std::env::set_var("HIVE_RUN_TESTS_BEFORE_MR", "1");
        let c = HiveConfig::from_env();
        std::env::remove_var("HIVE_RUN_TESTS_BEFORE_MR");
        assert!(c.run_tests_before_mr);
    }

    #[test]
    #[serial]
    fn from_env_protect_main_sets_default_integration() {
        std::env::set_var("HIVE_PROTECT_MAIN", "1");
        std::env::remove_var("HIVE_INTEGRATION_BRANCH");
        let c = HiveConfig::from_env();
        std::env::remove_var("HIVE_PROTECT_MAIN");
        assert!(c.protect_main);
        assert_eq!(c.integration_branch, "hive/integration");
        assert!(c.validate().is_ok());
    }

    #[test]
    #[serial]
    fn from_env_integration_branch_explicit() {
        std::env::set_var("HIVE_PROTECT_MAIN", "1");
        std::env::set_var("HIVE_INTEGRATION_BRANCH", "hive/queen-line");
        let c = HiveConfig::from_env();
        std::env::remove_var("HIVE_PROTECT_MAIN");
        std::env::remove_var("HIVE_INTEGRATION_BRANCH");
        assert_eq!(c.integration_branch, "hive/queen-line");
        assert!(c.validate().is_ok());
    }

    #[test]
    fn validate_rejects_main_as_integration_when_protected() {
        let c = HiveConfig {
            protect_main: true,
            integration_branch: "main".into(),
            ..HiveConfig::default()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn validate_rejects_master_as_integration_when_protected() {
        let c = HiveConfig {
            protect_main: true,
            integration_branch: "master".into(),
            ..HiveConfig::default()
        };
        assert!(c.validate().is_err());
    }
}
