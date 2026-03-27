//! Plugin marketplace — registro y descubrimiento de plugins.
//! Modela un marketplace local con metadata de plugins disponibles.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub language: String,
    pub capabilities: Vec<String>,
    pub entry_point: String,
    pub downloads: usize,
    pub rating: f32,
}

pub struct PluginMarketplace {
    plugins: HashMap<String, PluginManifest>,
    registry_path: PathBuf,
}

impl PluginMarketplace {
    pub fn new(repo_root: &Path) -> Self {
        let path = repo_root.join(".hive").join("plugin_registry.json");
        let plugins = Self::load(&path).unwrap_or_default();
        Self {
            plugins,
            registry_path: path,
        }
    }

    fn load(path: &Path) -> anyhow::Result<HashMap<String, PluginManifest>> {
        if !path.exists() {
            return Ok(HashMap::new());
        }
        let raw = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.registry_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.plugins)?;
        std::fs::write(&self.registry_path, json)?;
        Ok(())
    }

    pub fn register(&mut self, manifest: PluginManifest) {
        self.plugins.insert(manifest.name.clone(), manifest);
        let _ = self.save();
    }

    pub fn search(&self, query: &str) -> Vec<&PluginManifest> {
        self.plugins
            .values()
            .filter(|p| {
                p.name.contains(query)
                    || p.description.contains(query)
                    || p.language.contains(query)
            })
            .collect()
    }

    pub fn get(&self, name: &str) -> Option<&PluginManifest> {
        self.plugins.get(name)
    }

    pub fn list_all(&self) -> Vec<&PluginManifest> {
        self.plugins.values().collect()
    }

    pub fn list_by_language(&self, lang: &str) -> Vec<&PluginManifest> {
        self.plugins
            .values()
            .filter(|p| p.language == lang)
            .collect()
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let removed = self.plugins.remove(name).is_some();
        if removed {
            let _ = self.save();
        }
        removed
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_plugin(name: &str) -> PluginManifest {
        PluginManifest {
            name: name.into(),
            version: "1.0.0".into(),
            author: "test".into(),
            description: "A test plugin".into(),
            language: "rust".into(),
            capabilities: vec!["lint".into()],
            entry_point: "main.wasm".into(),
            downloads: 0,
            rating: 5.0,
        }
    }

    #[test]
    fn register_and_search() {
        let tmp = tempfile::tempdir().unwrap();
        let mut m = PluginMarketplace::new(tmp.path());
        m.register(sample_plugin("rust-linter"));
        assert_eq!(m.len(), 1);
        assert!(!m.search("linter").is_empty());
    }

    #[test]
    fn list_by_language() {
        let tmp = tempfile::tempdir().unwrap();
        let mut m = PluginMarketplace::new(tmp.path());
        m.register(sample_plugin("rust-linter"));
        let mut p2 = sample_plugin("py-linter");
        p2.language = "python".into();
        m.register(p2);
        assert_eq!(m.list_by_language("rust").len(), 1);
    }
}
