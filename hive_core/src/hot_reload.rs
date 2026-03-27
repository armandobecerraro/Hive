//! Hot reload de configuración.
//! Monitorea cambios en config y recarga sin reiniciar.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigValue {
    pub value: String,
    pub source: String,
    pub last_modified: i64,
}

pub struct HotReload {
    config_path: PathBuf,
    values: HashMap<String, ConfigValue>,
    last_check: i64,
}

impl HotReload {
    pub fn new(config_path: &Path) -> Self {
        Self {
            config_path: config_path.to_path_buf(),
            values: HashMap::new(),
            last_check: 0,
        }
    }

    /// Verifica si la config cambió y recarga si es necesario.
    pub fn check_and_reload(&mut self) -> bool {
        if let Ok(meta) = std::fs::metadata(&self.config_path) {
            if let Ok(modified) = meta.modified() {
                let ts = modified
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                if ts > self.last_check {
                    self.last_check = ts;
                    return self.reload();
                }
            }
        }
        false
    }

    fn reload(&mut self) -> bool {
        if let Ok(content) = std::fs::read_to_string(&self.config_path) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&content) {
                for (k, v) in map {
                    self.values.insert(
                        k,
                        ConfigValue {
                            value: v,
                            source: self.config_path.to_string_lossy().to_string(),
                            last_modified: self.last_check,
                        },
                    );
                }
                return true;
            }
        }
        false
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(|v| v.value.as_str())
    }

    pub fn all(&self) -> &HashMap<String, ConfigValue> {
        &self.values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_from_file() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.json");
        std::fs::write(&config_path, r#"{"key": "value"}"#).unwrap();

        let mut hr = HotReload::new(&config_path);
        assert!(hr.reload());
        assert_eq!(hr.get("key"), Some("value"));
    }
}
