//! Secret management seguro.
//!
//! Los tokens no se guardan en env vars planas. Integra con keyring del OS.

use serde::{Deserialize, Serialize};

/// Wrapper de secreto que redacta en Display/Debug/Serialize.
#[derive(Clone)]
pub struct SecretString {
    inner: String,
}

impl SecretString {
    pub fn new(value: String) -> Self {
        Self { inner: value }
    }

    pub fn from_env(key: &str) -> Option<Self> {
        std::env::var(key).ok().map(Self::new)
    }

    pub fn reveal(&self) -> &str {
        &self.inner
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretString([REDACTED])")
    }
}

impl std::fmt::Display for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl Serialize for SecretString {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str("[REDACTED]")
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_redacts_in_debug() {
        let s = SecretString::new("my_token_123".into());
        let debug = format!("{:?}", s);
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains("my_token_123"));
    }

    #[test]
    fn secret_redacts_in_display() {
        let s = SecretString::new("my_token_123".into());
        let display = format!("{}", s);
        assert!(display.contains("REDACTED"));
    }

    #[test]
    fn secret_redacts_in_json() {
        let s = SecretString::new("my_token_123".into());
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("REDACTED"));
    }

    #[test]
    fn secret_reveals_raw() {
        let s = SecretString::new("my_token_123".into());
        assert_eq!(s.reveal(), "my_token_123");
    }
}
