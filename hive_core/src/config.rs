//! Configuración por variables de entorno (documentadas en README o comentarios).

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
    /// Tras tantos rechazos del Consejo sobre la misma obrera, se **descarta** el trabajo (borra rama, vuelve a `main`) y la cola **sigue** con otras tareas (evita bucles infinitos).
    pub max_mr_rejection_attempts: u32,
    /// Si es true, antes de enviar el MR al Consejo ejecuta `cargo test` en repos con `Cargo.toml` (fallo → no se abre MR y se reintenta como rechazo lógico vía error en la obrera).
    pub run_tests_before_mr: bool,
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
        }
    }
}

impl HiveConfig {
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

        let maintainer_reject_before_approve = std::env::var("HIVE_MAINTAINER_REJECT_BEFORE_APPROVE")
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

        Self {
            poll_interval: Duration::from_secs(poll_secs),
            max_decision_records,
            max_version_history_entries,
            maintainer_reject_before_approve,
            max_mr_rejection_attempts,
            run_tests_before_mr,
        }
    }

    /// `HIVE_DAEMON=1` o `true` activa bucle en el binario (también existe `--daemon`).
    pub fn daemon_from_env() -> bool {
        match std::env::var("HIVE_DAEMON").as_deref() {
            Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES") => true,
            _ => false,
        }
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
}
