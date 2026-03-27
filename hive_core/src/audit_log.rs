//! Audit logging para compliance.
//! Registra quién hizo qué y cuándo.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: i64,
    pub action: String,
    pub actor: String,
    pub target: String,
    pub details: String,
    pub success: bool,
}

pub struct AuditLog {
    entries: Vec<AuditEntry>,
    path: PathBuf,
}

impl AuditLog {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            entries: Vec::new(),
            path: repo_root.join(".hive").join("audit.jsonl"),
        }
    }

    pub fn log(&mut self, action: &str, actor: &str, target: &str, details: &str, success: bool) {
        let entry = AuditEntry {
            timestamp: chrono::Utc::now().timestamp(),
            action: action.into(),
            actor: actor.into(),
            target: target.into(),
            details: details.into(),
            success,
        };
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let line = format!("{}\n", serde_json::to_string(&entry).unwrap_or_default());
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .and_then(|mut f| {
                use std::io::Write;
                f.write_all(line.as_bytes())
            });
        self.entries.push(entry);
    }

    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }
}
