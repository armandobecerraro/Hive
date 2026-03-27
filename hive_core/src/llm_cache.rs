//! Caching de respuestas LLM.
//!
//! Cachea respuestas para ahorrar tokens en consultas repetidas.
//! Inspirado en GPTCache y LangChain caching.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Entrada del cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub prompt_hash: String,
    pub response: String,
    pub model: String,
    pub tokens_saved: usize,
    pub timestamp: i64,
    pub hits: usize,
}

/// Cache de respuestas LLM.
pub struct LlmCache {
    entries: HashMap<String, CacheEntry>,
    path: PathBuf,
    max_entries: usize,
    ttl_secs: i64,
}

impl LlmCache {
    pub fn new(repo_root: &Path, max_entries: usize, ttl_secs: i64) -> Self {
        let path = repo_root.join(".hive").join("llm_cache.json");
        let entries = Self::load_from_disk(&path).unwrap_or_default();
        Self {
            entries,
            path,
            max_entries,
            ttl_secs,
        }
    }

    fn load_from_disk(path: &Path) -> anyhow::Result<HashMap<String, CacheEntry>> {
        if !path.exists() {
            return Ok(HashMap::new());
        }
        let raw = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.entries)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    fn hash_prompt(prompt: &str, model: &str) -> String {
        let mut hash = 0u64;
        for byte in format!("{model}:{prompt}").bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
        format!("{hash:016x}")
    }

    /// Busca en cache.
    pub fn get(&mut self, prompt: &str, model: &str) -> Option<String> {
        let key = Self::hash_prompt(prompt, model);
        let now = chrono::Utc::now().timestamp();

        if let Some(entry) = self.entries.get_mut(&key) {
            if now - entry.timestamp < self.ttl_secs {
                entry.hits += 1;
                return Some(entry.response.clone());
            }
            self.entries.remove(&key);
        }
        None
    }

    /// Inserta en cache.
    pub fn put(&mut self, prompt: &str, model: &str, response: &str, tokens: usize) {
        if self.entries.len() >= self.max_entries {
            // Remover el más antiguo
            if let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.timestamp)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&oldest_key);
            }
        }

        let key = Self::hash_prompt(prompt, model);
        self.entries.insert(
            key,
            CacheEntry {
                prompt_hash: Self::hash_prompt(prompt, ""),
                response: response.to_string(),
                model: model.to_string(),
                tokens_saved: tokens,
                timestamp: chrono::Utc::now().timestamp(),
                hits: 0,
            },
        );
        let _ = self.save();
    }

    /// Estadísticas del cache.
    pub fn stats(&self) -> CacheStats {
        let total_hits: usize = self.entries.values().map(|e| e.hits).sum();
        let tokens_saved: usize = self
            .entries
            .values()
            .map(|e| e.tokens_saved * e.hits.max(1))
            .sum();
        CacheStats {
            entries: self.entries.len(),
            total_hits,
            tokens_saved,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    pub entries: usize,
    pub total_hits: usize,
    pub tokens_saved: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_put_and_get() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cache = LlmCache::new(tmp.path(), 100, 3600);
        cache.put("hello", "gpt-4", "response", 50);
        assert_eq!(cache.get("hello", "gpt-4"), Some("response".into()));
    }

    #[test]
    fn cache_miss_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cache = LlmCache::new(tmp.path(), 100, 3600);
        assert_eq!(cache.get("nonexistent", "gpt-4"), None);
    }
}
