//! Vector store real con embeddings para RAG (Retrieval-Augmented Generation).
//!
//! Implementa almacenamiento de vectores con similitud coseno para búsqueda semántica.
//! Soporta embeddings pre-computados y búsqueda por vecinos más cercanos.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Entrada del vector store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorEntry {
    pub id: String,
    pub embedding: Vec<f32>,
    pub content: String,
    pub metadata: HashMap<String, String>,
    pub timestamp: i64,
}

/// Resultado de búsqueda semántica.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub entry_id: String,
    pub content: String,
    pub similarity: f32,
    pub metadata: HashMap<String, String>,
}

/// Configuración del vector store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorStoreConfig {
    pub embedding_dim: usize,
    pub max_entries: usize,
    pub similarity_threshold: f32,
}

impl Default for VectorStoreConfig {
    fn default() -> Self {
        Self {
            embedding_dim: 384,
            max_entries: 10_000,
            similarity_threshold: 0.3,
        }
    }
}

/// Vector store basado en similitud coseno.
pub struct VectorStore {
    entries: Vec<VectorEntry>,
    config: VectorStoreConfig,
    path: PathBuf,
}

impl VectorStore {
    pub fn new(repo_root: &Path, config: VectorStoreConfig) -> Self {
        let path = repo_root.join(".hive").join("vector_store.json");
        let entries = Self::load_from_disk(&path).unwrap_or_default();
        Self {
            entries,
            config,
            path,
        }
    }

    fn load_from_disk(path: &Path) -> anyhow::Result<Vec<VectorEntry>> {
        if !path.exists() {
            return Ok(Vec::new());
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

    /// Inserta una entrada con embedding pre-computado.
    pub fn insert(
        &mut self,
        id: String,
        embedding: Vec<f32>,
        content: String,
        metadata: HashMap<String, String>,
    ) {
        if self.entries.len() >= self.config.max_entries {
            self.entries.remove(0);
        }
        self.entries.push(VectorEntry {
            id,
            embedding,
            content,
            metadata,
            timestamp: chrono::Utc::now().timestamp(),
        });
        let _ = self.save();
    }

    /// Genera un embedding simple basado en TF-IDF simplificado (hash-based).
    /// Para producción se conectaría a un modelo real (sentence-transformers, OpenAI).
    pub fn compute_embedding(text: &str, dim: usize) -> Vec<f32> {
        let mut embedding = vec![0.0f32; dim];
        let words: Vec<&str> = text.split_whitespace().collect();
        let total = words.len() as f32;
        if total == 0.0 {
            return embedding;
        }

        let mut word_counts: HashMap<&str, usize> = HashMap::new();
        for word in &words {
            *word_counts.entry(word).or_insert(0) += 1;
        }

        for (word, count) in &word_counts {
            let hash = Self::hash_word(word) % dim;
            embedding[hash] += (*count as f32) / total;
        }

        // Normalizar
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in embedding.iter_mut() {
                *x /= norm;
            }
        }
        embedding
    }

    fn hash_word(word: &str) -> usize {
        let mut hash = 0usize;
        for byte in word.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as usize);
        }
        hash
    }

    /// Búsqueda por similitud coseno.
    pub fn search(&self, query_embedding: &[f32], limit: usize) -> Vec<SearchResult> {
        let mut results: Vec<SearchResult> = self
            .entries
            .iter()
            .map(|entry| {
                let similarity = cosine_similarity(query_embedding, &entry.embedding);
                SearchResult {
                    entry_id: entry.id.clone(),
                    content: entry.content.clone(),
                    similarity,
                    metadata: entry.metadata.clone(),
                }
            })
            .filter(|r| r.similarity >= self.config.similarity_threshold)
            .collect();

        results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);
        results
    }

    /// Búsqueda por texto (genera embedding del query y busca).
    pub fn search_text(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let embedding = Self::compute_embedding(query, self.config.embedding_dim);
        self.search(&embedding, limit)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_embedding_deterministic() {
        let e1 = VectorStore::compute_embedding("hello world", 128);
        let e2 = VectorStore::compute_embedding("hello world", 128);
        assert_eq!(e1, e2);
    }

    #[test]
    fn cosine_similarity_identical_is_one() {
        let v = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_orthogonal_is_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 0.001);
    }

    #[test]
    fn vector_store_insert_and_search() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = VectorStore::new(tmp.path(), VectorStoreConfig::default());
        let emb = VectorStore::compute_embedding("rust programming language", 384);
        store.insert("1".into(), emb, "fn main() {}".into(), HashMap::new());
        assert_eq!(store.len(), 1);

        let results = store.search_text("rust code", 5);
        assert!(!results.is_empty());
    }
}
