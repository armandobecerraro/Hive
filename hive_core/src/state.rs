use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HiveState {
    pub schema_version: u32,
    #[serde(default)]
    pub decision_tree: Vec<DecisionRecord>,
    #[serde(default)]
    pub version_history: Vec<VersionHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub at: DateTime<Utc>,
    pub decision: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionHistoryEntry {
    pub at: DateTime<Utc>,
    pub version: String,
    pub merge_request_id: Uuid,
    pub merged_branch: String,
}

impl Default for HiveState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            decision_tree: Vec::new(),
            version_history: Vec::new(),
        }
    }
}

impl HiveState {
    pub fn path_for_repo(repo_root: &Path) -> PathBuf {
        repo_root.join("hive.json")
    }

    pub fn load_or_new(repo_root: &Path) -> Result<Self> {
        let p = Self::path_for_repo(repo_root);
        if !p.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&p).with_context(|| format!("leer {}", p.display()))?;
        let s: HiveState = serde_json::from_str(&raw).with_context(|| "parsear hive.json")?;
        Ok(s)
    }

    pub fn save(&self, repo_root: &Path) -> Result<()> {
        let p = Self::path_for_repo(repo_root);
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(&p, raw).with_context(|| format!("escribir {}", p.display()))?;
        Ok(())
    }

    pub fn log_decision(&mut self, decision: impl Into<String>, detail: impl Into<String>) {
        self.decision_tree.push(DecisionRecord {
            at: Utc::now(),
            decision: decision.into(),
            detail: detail.into(),
        });
    }

    pub fn record_merge(
        &mut self,
        version: impl Into<String>,
        merge_request_id: Uuid,
        merged_branch: impl Into<String>,
    ) {
        self.version_history.push(VersionHistoryEntry {
            at: Utc::now(),
            version: version.into(),
            merge_request_id,
            merged_branch: merged_branch.into(),
        });
    }

    /// Recorta listas persistidas para evitar crecimiento ilimitado de `hive.json`.
    pub fn trim_retention(&mut self, max_decisions: usize, max_versions: usize) {
        if self.decision_tree.len() > max_decisions {
            let excess = self.decision_tree.len() - max_decisions;
            self.decision_tree.drain(..excess);
        }
        if self.version_history.len() > max_versions {
            let excess = self.version_history.len() - max_versions;
            self.version_history.drain(..excess);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn hive_state_default_y_path() {
        let d = tempfile::tempdir().unwrap();
        let p = HiveState::path_for_repo(d.path());
        assert!(p.ends_with("hive.json"));
    }

    #[test]
    fn load_or_new_sin_archivo() {
        let d = tempfile::tempdir().unwrap();
        let s = HiveState::load_or_new(d.path()).unwrap();
        assert_eq!(s.schema_version, 1);
        assert!(s.decision_tree.is_empty());
    }

    #[test]
    fn save_load_roundtrip() {
        let d = tempfile::tempdir().unwrap();
        let mut s = HiveState::default();
        s.log_decision("a", "det");
        s.record_merge("1.0.1", Uuid::nil(), "hive/w");
        s.save(d.path()).unwrap();
        let s2 = HiveState::load_or_new(d.path()).unwrap();
        assert_eq!(s2.decision_tree.len(), 1);
        assert_eq!(s2.version_history.len(), 1);
        assert_eq!(s2.version_history[0].version, "1.0.1");
    }

    #[test]
    fn load_json_invalido_falla() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("hive.json"), "not json {{{").unwrap();
        assert!(HiveState::load_or_new(d.path()).is_err());
    }

    #[test]
    fn trim_retention_mantiene_cola_corta() {
        let mut s = HiveState::default();
        for i in 0..5 {
            s.log_decision("d", format!("{i}"));
        }
        s.record_merge("1.0.0", Uuid::nil(), "b1");
        s.record_merge("1.0.1", Uuid::nil(), "b2");
        s.record_merge("1.0.2", Uuid::nil(), "b3");
        s.trim_retention(2, 2);
        assert_eq!(s.decision_tree.len(), 2);
        assert_eq!(s.decision_tree[0].detail, "3");
        assert_eq!(s.version_history.len(), 2);
    }

    #[test]
    fn trim_retention_recorta_solo_historial_de_versiones() {
        let mut s = HiveState::default();
        for i in 0..5 {
            s.record_merge(format!("1.0.{i}"), Uuid::nil(), format!("b{i}"));
        }
        s.trim_retention(100, 2);
        assert_eq!(s.version_history.len(), 2);
        assert_eq!(s.version_history[0].merged_branch, "b3");
        assert_eq!(s.version_history[1].merged_branch, "b4");
    }
}
