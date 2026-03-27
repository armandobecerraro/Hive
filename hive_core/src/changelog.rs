//! Generación de changelog automático.
//!
//! Genera CHANGELOG.md automáticamente desde commits semánticos.
//! Inspirado en standard-version y release-please.

use serde::{Deserialize, Serialize};

/// Entrada del changelog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogEntry {
    pub version: String,
    pub date: String,
    pub features: Vec<String>,
    pub fixes: Vec<String>,
    pub breaking: Vec<String>,
    pub other: Vec<String>,
}

/// Generador de changelog.
pub struct ChangelogGenerator;

impl ChangelogGenerator {
    /// Genera una entrada de changelog desde commits.
    pub fn generate_entry(version: &str, commits: &[&str]) -> ChangelogEntry {
        let mut entry = ChangelogEntry {
            version: version.into(),
            date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
            features: Vec::new(),
            fixes: Vec::new(),
            breaking: Vec::new(),
            other: Vec::new(),
        };

        for commit in commits {
            if commit.starts_with("feat!") || commit.contains("BREAKING") {
                entry.breaking.push(commit.to_string());
            } else if commit.starts_with("feat") {
                entry.features.push(commit.to_string());
            } else if commit.starts_with("fix") {
                entry.fixes.push(commit.to_string());
            } else {
                entry.other.push(commit.to_string());
            }
        }

        entry
    }

    /// Formatea una entrada como Markdown.
    pub fn format_entry(entry: &ChangelogEntry) -> String {
        let mut md = format!("## [{}] - {}\n\n", entry.version, entry.date);

        if !entry.breaking.is_empty() {
            md.push_str("### ⚠ BREAKING CHANGES\n\n");
            for item in &entry.breaking {
                md.push_str(&format!("- {item}\n"));
            }
            md.push('\n');
        }
        if !entry.features.is_empty() {
            md.push_str("### Features\n\n");
            for item in &entry.features {
                md.push_str(&format!("- {item}\n"));
            }
            md.push('\n');
        }
        if !entry.fixes.is_empty() {
            md.push_str("### Bug Fixes\n\n");
            for item in &entry.fixes {
                md.push_str(&format!("- {item}\n"));
            }
            md.push('\n');
        }
        if !entry.other.is_empty() {
            md.push_str("### Other\n\n");
            for item in &entry.other {
                md.push_str(&format!("- {item}\n"));
            }
            md.push('\n');
        }

        md
    }

    /// Genera un CHANGELOG.md completo.
    pub fn generate_full(entries: &[ChangelogEntry]) -> String {
        let mut md = String::from("# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n");
        for entry in entries {
            md.push_str(&Self::format_entry(entry));
        }
        md
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_entry_categorizes() {
        let commits = ["feat: add api", "fix: resolve crash", "chore: update deps"];
        let entry = ChangelogGenerator::generate_entry("1.0.0", &commits);
        assert_eq!(entry.features.len(), 1);
        assert_eq!(entry.fixes.len(), 1);
    }

    #[test]
    fn format_entry_produces_md() {
        let entry = ChangelogGenerator::generate_entry("1.0.0", &["feat: test"]);
        let md = ChangelogGenerator::format_entry(&entry);
        assert!(md.contains("## [1.0.0]"));
        assert!(md.contains("### Features"));
    }
}
