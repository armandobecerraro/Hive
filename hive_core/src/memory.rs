//! Memoria persistente RAG — vector store para contexto acumulado.
//!
//! Las obreras consultan decisiones pasadas y patrones similares antes de actuar.
//! Inspirado en el sistema de memoria de CrewAI (short-term, long-term, entity).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Entrada de memoria (decisión o patrón aprendido).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub timestamp: i64,
    pub entry_type: MemoryType,
    pub specialist_key: String,
    pub task_description: String,
    pub solution: String,
    pub tags: Vec<String>,
    pub relevance_score: f32,
    pub context: HashMap<String, String>,
}

/// Tipo de entrada de memoria.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemoryType {
    /// Decisión tomada por una obrera.
    Decision,
    /// Solución a un problema específico.
    Solution,
    /// Patrón de código reutilizable.
    CodePattern,
    /// Error encontrado y su resolución.
    ErrorResolution,
    /// Feedback del Consejo.
    CouncilFeedback,
    /// Configuración que funcionó.
    ConfigSuccess,
}

/// Vector store simplificado basado en similitud de texto (sin embeddings por ahora).
/// Para producción se conectaría a Qdrant/Milvus.
pub struct MemoryStore {
    entries: Vec<MemoryEntry>,
    path: PathBuf,
    max_entries: usize,
}

impl MemoryStore {
    pub fn new(repo_root: &Path, max_entries: usize) -> Self {
        let path = repo_root.join(".hive").join("memory.json");
        let entries = Self::load_from_disk(&path).unwrap_or_default();
        Self {
            entries,
            path,
            max_entries,
        }
    }

    fn load_from_disk(path: &Path) -> Result<Vec<MemoryEntry>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let raw = std::fs::read_to_string(path)?;
        let entries: Vec<MemoryEntry> = serde_json::from_str(&raw)?;
        Ok(entries)
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.entries)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    /// Añade una entrada a la memoria.
    pub fn store(&mut self, entry: MemoryEntry) {
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        if let Err(e) = self.save() {
            warn!(error = %e, "error persistiendo memoria a disco");
        }
    }

    /// Busca entradas similares por similitud de texto (Jaccard simplificado).
    pub fn recall(&self, query: &str, specialist: Option<&str>, limit: usize) -> Vec<&MemoryEntry> {
        let query_words: std::collections::HashSet<&str> = query.split_whitespace().collect();

        let mut scored: Vec<(&MemoryEntry, f32)> = self
            .entries
            .iter()
            .filter(|e| specialist.is_none_or(|s| e.specialist_key == s))
            .map(|entry| {
                let desc_words: std::collections::HashSet<&str> =
                    entry.task_description.split_whitespace().collect();
                let sol_words: std::collections::HashSet<&str> =
                    entry.solution.split_whitespace().collect();
                let all_words: std::collections::HashSet<&str> =
                    desc_words.union(&sol_words).copied().collect();

                let intersection = query_words.intersection(&all_words).count();
                let union = query_words.union(&all_words).count();
                let score = if union > 0 {
                    intersection as f32 / union as f32
                } else {
                    0.0
                };
                (entry, score)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        scored.into_iter().map(|(e, _)| e).collect()
    }

    /// Busca por tags.
    pub fn search_by_tags(&self, tags: &[String]) -> Vec<&MemoryEntry> {
        self.entries
            .iter()
            .filter(|e| tags.iter().any(|t| e.tags.contains(t)))
            .collect()
    }

    /// Busca por tipo.
    pub fn search_by_type(&self, entry_type: &MemoryType) -> Vec<&MemoryEntry> {
        self.entries
            .iter()
            .filter(|e| &e.entry_type == entry_type)
            .collect()
    }

    /// Estadísticas de la memoria.
    pub fn stats(&self) -> MemoryStats {
        let mut by_type: HashMap<String, usize> = HashMap::new();
        let mut by_specialist: HashMap<String, usize> = HashMap::new();

        for entry in &self.entries {
            let type_key = format!("{:?}", entry.entry_type);
            *by_type.entry(type_key).or_insert(0) += 1;
            *by_specialist
                .entry(entry.specialist_key.clone())
                .or_insert(0) += 1;
        }

        MemoryStats {
            total_entries: self.entries.len(),
            by_type,
            by_specialist,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Estadísticas de la memoria.
#[derive(Debug, Clone, Serialize)]
pub struct MemoryStats {
    pub total_entries: usize,
    pub by_type: HashMap<String, usize>,
    pub by_specialist: HashMap<String, usize>,
}

/// Constructor rápido de MemoryEntry.
pub fn build_memory_entry(
    entry_type: MemoryType,
    specialist: &str,
    task: &str,
    solution: &str,
    tags: Vec<String>,
) -> MemoryEntry {
    MemoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().timestamp(),
        entry_type,
        specialist_key: specialist.to_string(),
        task_description: task.to_string(),
        solution: solution.to_string(),
        tags,
        relevance_score: 1.0,
        context: HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry() -> MemoryEntry {
        build_memory_entry(
            MemoryType::Solution,
            "rust",
            "Resolver merge conflict en Cargo.toml",
            "Usar ours/theirs strategy",
            vec!["git".into(), "merge".into()],
        )
    }

    #[test]
    fn memory_store_add_and_recall() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::new(tmp.path(), 100);
        store.store(sample_entry());

        let results = store.recall("merge conflict", None, 5);
        assert!(!results.is_empty());
    }

    #[test]
    fn memory_store_search_by_tags() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::new(tmp.path(), 100);
        store.store(sample_entry());

        let results = store.search_by_tags(&["git".into()]);
        assert!(!results.is_empty());
    }

    #[test]
    fn memory_store_search_by_type() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::new(tmp.path(), 100);
        store.store(sample_entry());

        let results = store.search_by_type(&MemoryType::Solution);
        assert!(!results.is_empty());
    }

    #[test]
    fn memory_store_stats() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::new(tmp.path(), 100);
        store.store(sample_entry());

        let stats = store.stats();
        assert_eq!(stats.total_entries, 1);
    }

    #[test]
    fn memory_store_persists() {
        let tmp = tempfile::tempdir().unwrap();
        {
            let mut store = MemoryStore::new(tmp.path(), 100);
            store.store(sample_entry());
        }
        let store = MemoryStore::new(tmp.path(), 100);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn memory_store_max_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::new(tmp.path(), 3);
        for i in 0..5 {
            store.store(build_memory_entry(
                MemoryType::Decision,
                "test",
                &format!("task {i}"),
                "solution",
                vec![],
            ));
        }
        assert_eq!(store.len(), 3);
    }
}
