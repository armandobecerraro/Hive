//! ADR (Architecture Decision Records).
//! Registro de decisiones de diseño del proyecto.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Adr {
    pub id: usize,
    pub title: String,
    pub status: AdrStatus,
    pub context: String,
    pub decision: String,
    pub consequences: String,
    pub date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdrStatus {
    Proposed,
    Accepted,
    Deprecated,
    Superseded,
}

pub struct AdrManager {
    path: PathBuf,
}

impl AdrManager {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            path: repo_root.join("docs").join("adr"),
        }
    }

    pub fn create(&self, title: &str, context: &str, decision: &str, consequences: &str) -> Adr {
        let _ = std::fs::create_dir_all(&self.path);
        let id = self.next_id();
        let adr = Adr {
            id,
            title: title.into(),
            status: AdrStatus::Accepted,
            context: context.into(),
            decision: decision.into(),
            consequences: consequences.into(),
            date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        };

        let filename = format!("{:04}-{}.md", id, title.to_lowercase().replace(' ', "-"));
        let content = format!(
            "# ADR-{id:04}: {title}\n\n**Status:** {:?}\n**Date:** {}\n\n## Context\n{context}\n\n## Decision\n{decision}\n\n## Consequences\n{consequences}\n",
            adr.status, adr.date
        );
        let _ = std::fs::write(self.path.join(filename), content);
        adr
    }

    fn next_id(&self) -> usize {
        if !self.path.exists() {
            return 1;
        }
        std::fs::read_dir(&self.path)
            .map(|entries| entries.count() + 1)
            .unwrap_or(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adr_create_works() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = AdrManager::new(tmp.path());
        let adr = mgr.create(
            "Use Rust",
            "Need fast runtime",
            "Use Rust",
            "Learning curve",
        );
        assert_eq!(adr.id, 1);
    }
}
