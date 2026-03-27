//! Snapshot testing para verificar outputs contra snapshots guardados.
//! Inspirado en insta crate.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub name: String,
    pub content: String,
    pub timestamp: i64,
}

pub struct SnapshotStore {
    path: PathBuf,
    snapshots: HashMap<String, Snapshot>,
}

impl SnapshotStore {
    pub fn new(repo_root: &Path) -> Self {
        let path = repo_root.join(".hive").join("snapshots.json");
        let snapshots = Self::load(&path).unwrap_or_default();
        Self { path, snapshots }
    }

    fn load(path: &Path) -> anyhow::Result<HashMap<String, Snapshot>> {
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
        let json = serde_json::to_string_pretty(&self.snapshots)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    /// Guarda o actualiza un snapshot.
    pub fn record(&mut self, name: &str, content: &str) {
        self.snapshots.insert(
            name.into(),
            Snapshot {
                name: name.into(),
                content: content.into(),
                timestamp: chrono::Utc::now().timestamp(),
            },
        );
        let _ = self.save();
    }

    /// Verifica que el contenido actual coincide con el snapshot.
    pub fn assert_snapshot(&self, name: &str, actual: &str) -> SnapshotResult {
        match self.snapshots.get(name) {
            Some(snapshot) => {
                if snapshot.content == actual {
                    SnapshotResult::Match
                } else {
                    SnapshotResult::Mismatch {
                        expected: snapshot.content.clone(),
                        actual: actual.to_string(),
                    }
                }
            }
            None => SnapshotResult::NewSnapshot {
                name: name.into(),
                content: actual.to_string(),
            },
        }
    }

    /// Actualiza un snapshot existente.
    pub fn update(&mut self, name: &str, content: &str) {
        self.record(name, content);
    }

    pub fn len(&self) -> usize {
        self.snapshots.len()
    }
}

#[derive(Debug, Clone)]
pub enum SnapshotResult {
    Match,
    Mismatch { expected: String, actual: String },
    NewSnapshot { name: String, content: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_assert_match() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = SnapshotStore::new(tmp.path());
        store.record("test", "hello world");
        assert!(matches!(
            store.assert_snapshot("test", "hello world"),
            SnapshotResult::Match
        ));
    }

    #[test]
    fn assert_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = SnapshotStore::new(tmp.path());
        store.record("test", "hello");
        match store.assert_snapshot("test", "world") {
            SnapshotResult::Mismatch { .. } => {}
            _ => panic!("expected mismatch"),
        }
    }

    #[test]
    fn new_snapshot_detected() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SnapshotStore::new(tmp.path());
        match store.assert_snapshot("new", "content") {
            SnapshotResult::NewSnapshot { .. } => {}
            _ => panic!("expected new snapshot"),
        }
    }
}
