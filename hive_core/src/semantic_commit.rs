//! Generación semántica de commits.
//!
//! Genera mensajes de commit basados en el diff, siguiendo conventional commits.
//! Inspirado en Copilot y herramientas de conventional commits.

use serde::{Deserialize, Serialize};

/// Tipo de commit conventional.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommitType {
    Feat,
    Fix,
    Docs,
    Style,
    Refactor,
    Perf,
    Test,
    Build,
    Ci,
    Chore,
    Revert,
}

impl std::fmt::Display for CommitType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Feat => write!(f, "feat"),
            Self::Fix => write!(f, "fix"),
            Self::Docs => write!(f, "docs"),
            Self::Style => write!(f, "style"),
            Self::Refactor => write!(f, "refactor"),
            Self::Perf => write!(f, "perf"),
            Self::Test => write!(f, "test"),
            Self::Build => write!(f, "build"),
            Self::Ci => write!(f, "ci"),
            Self::Chore => write!(f, "chore"),
            Self::Revert => write!(f, "revert"),
        }
    }
}

/// Mensaje de commit generado.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitMessage {
    pub commit_type: CommitType,
    pub scope: Option<String>,
    pub description: String,
    pub body: Option<String>,
    pub breaking: bool,
}

impl std::fmt::Display for CommitMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let scope = self
            .scope
            .as_ref()
            .map(|s| format!("({s})"))
            .unwrap_or_default();
        let breaking = if self.breaking { "!" } else { "" };
        write!(
            f,
            "{}{}{}: {}",
            self.commit_type, scope, breaking, self.description
        )?;
        if let Some(ref body) = self.body {
            write!(f, "\n\n{body}")?;
        }
        Ok(())
    }
}

/// Generador de commits semánticos.
pub struct CommitMessageGenerator;

impl CommitMessageGenerator {
    /// Genera un mensaje de commit basado en el diff.
    pub fn generate(files_changed: &[String], diff: &str) -> CommitMessage {
        let commit_type = Self::infer_type(files_changed, diff);
        let scope = Self::infer_scope(files_changed);
        let description = Self::generate_description(&commit_type, files_changed, diff);

        CommitMessage {
            commit_type,
            scope,
            description,
            body: None,
            breaking: false,
        }
    }

    fn infer_type(files: &[String], diff: &str) -> CommitType {
        let added_lines = diff
            .lines()
            .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
            .count();
        let removed_lines = diff
            .lines()
            .filter(|l| l.starts_with('-') && !l.starts_with("---"))
            .count();

        if files.iter().any(|f| f.contains("test")) && added_lines > removed_lines {
            return CommitType::Test;
        }
        if files
            .iter()
            .any(|f| f.contains("README") || f.contains("docs"))
        {
            return CommitType::Docs;
        }
        if files
            .iter()
            .any(|f| f.contains(".github") || f.contains("ci"))
        {
            return CommitType::Ci;
        }
        if removed_lines > added_lines * 2 {
            return CommitType::Refactor;
        }
        if diff.contains("fix") || diff.contains("bug") || diff.contains("error") {
            return CommitType::Fix;
        }
        CommitType::Feat
    }

    fn infer_scope(files: &[String]) -> Option<String> {
        if files.len() == 1 {
            let file = &files[0];
            if let Some(name) = file.split('/').next_back() {
                return Some(
                    name.replace(".rs", "")
                        .replace(".py", "")
                        .replace(".js", ""),
                );
            }
        }
        // Inferir del directorio común
        let dirs: Vec<&str> = files.iter().filter_map(|f| f.split('/').nth(0)).collect();
        if dirs.len() == 1 {
            Some(dirs[0].to_string())
        } else {
            None
        }
    }

    fn generate_description(commit_type: &CommitType, files: &[String], _diff: &str) -> String {
        match commit_type {
            CommitType::Feat => format!("add functionality to {}", Self::file_summary(files)),
            CommitType::Fix => format!("resolve issue in {}", Self::file_summary(files)),
            CommitType::Docs => "update documentation".to_string(),
            CommitType::Test => format!("add tests for {}", Self::file_summary(files)),
            CommitType::Refactor => format!("refactor {}", Self::file_summary(files)),
            _ => format!("update {}", Self::file_summary(files)),
        }
    }

    fn file_summary(files: &[String]) -> String {
        if files.len() == 1 {
            files[0]
                .split('/')
                .next_back()
                .unwrap_or(&files[0])
                .to_string()
        } else {
            format!("{} files", files.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_feat_commit() {
        let msg = CommitMessageGenerator::generate(
            &["src/new_feature.rs".into()],
            "+fn new_feature() {}",
        );
        assert!(msg.to_string().starts_with("feat"));
    }

    #[test]
    fn generate_test_commit() {
        let msg = CommitMessageGenerator::generate(&["tests/test_main.rs".into()], "+#[test]");
        assert!(msg.to_string().starts_with("test"));
    }

    #[test]
    fn commit_message_display() {
        let msg = CommitMessage {
            commit_type: CommitType::Feat,
            scope: Some("api".into()),
            description: "add endpoint".into(),
            body: None,
            breaking: false,
        };
        assert_eq!(msg.to_string(), "feat(api): add endpoint");
    }
}
