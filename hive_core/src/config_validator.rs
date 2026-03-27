//! Configuración con validación estricta.
//! Valida env vars contra esquemas antes de usarlos.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatedConfig {
    pub value: String,
    pub validated: bool,
    pub error: Option<String>,
}

pub struct ConfigValidator;

impl ConfigValidator {
    pub fn validate_bool(key: &str) -> ValidatedConfig {
        let raw = std::env::var(key).unwrap_or_default();
        let validated =
            raw.is_empty() || matches!(raw.as_str(), "0" | "1" | "true" | "false" | "yes" | "no");
        ValidatedConfig {
            value: raw.clone(),
            validated,
            error: if !validated {
                Some(format!("{key} debe ser 0/1/true/false/yes/no"))
            } else {
                None
            },
        }
    }

    pub fn validate_numeric(key: &str, min: u64, max: u64) -> ValidatedConfig {
        let raw = std::env::var(key).unwrap_or_default();
        if raw.is_empty() {
            return ValidatedConfig {
                value: raw,
                validated: true,
                error: None,
            };
        }
        match raw.parse::<u64>() {
            Ok(n) if n >= min && n <= max => ValidatedConfig {
                value: raw,
                validated: true,
                error: None,
            },
            Ok(n) => ValidatedConfig {
                value: raw,
                validated: false,
                error: Some(format!("{key}={n} fuera de rango [{min},{max}]")),
            },
            Err(_) => ValidatedConfig {
                value: raw,
                validated: false,
                error: Some(format!("{key} no es numérico")),
            },
        }
    }

    pub fn validate_branch_name(key: &str) -> ValidatedConfig {
        let raw = std::env::var(key).unwrap_or_default();
        let valid = raw.is_empty()
            || raw
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/');
        ValidatedConfig {
            value: raw.clone(),
            validated: valid,
            error: if !valid {
                Some(format!("{key} contiene caracteres inválidos"))
            } else {
                None
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_bool_accepts_known() {
        std::env::set_var("TEST_BOOL_VALID", "true");
        let v = ConfigValidator::validate_bool("TEST_BOOL_VALID");
        assert!(v.validated);
        std::env::remove_var("TEST_BOOL_VALID");
    }

    #[test]
    fn validate_numeric_rejects_out_of_range() {
        std::env::set_var("TEST_NUM", "9999");
        let v = ConfigValidator::validate_numeric("TEST_NUM", 0, 100);
        assert!(!v.validated);
        std::env::remove_var("TEST_NUM");
    }
}
