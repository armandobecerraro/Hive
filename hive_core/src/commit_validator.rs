//! Validación de conventional commits.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitValidation {
    pub valid: bool,
    pub commit_type: Option<String>,
    pub scope: Option<String>,
    pub breaking: bool,
    pub errors: Vec<String>,
}

pub struct CommitValidator;

impl CommitValidator {
    pub fn validate(message: &str) -> CommitValidation {
        let first_line = message.lines().next().unwrap_or("");
        let mut errors = Vec::new();

        // Formato: type(scope)!: description
        let (type_part, rest) = if let Some(pos) = first_line.find('(') {
            (&first_line[..pos], &first_line[pos..])
        } else if let Some(pos) = first_line.find(':') {
            (&first_line[..pos], &first_line[pos..])
        } else {
            errors.push("Formato inválido: falta ':' después del tipo".into());
            return CommitValidation {
                valid: false,
                commit_type: None,
                scope: None,
                breaking: false,
                errors,
            };
        };

        let valid_types = [
            "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore",
            "revert",
        ];
        let commit_type = type_part.trim().trim_end_matches('!');

        if !valid_types.contains(&commit_type) {
            errors.push(format!(
                "Tipo '{commit_type}' no válido. Válidos: {}",
                valid_types.join(", ")
            ));
        }

        let breaking = first_line.contains('!') || message.contains("BREAKING CHANGE");
        let scope = if rest.starts_with('(') {
            rest[1..].find(')').map(|end| rest[1..end + 1].to_string())
        } else {
            None
        };

        if rest.contains(':')
            && rest
                .split(':')
                .nth(1)
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
        {
            errors.push("Descripción vacía después de ':'".into());
        }

        CommitValidation {
            valid: errors.is_empty(),
            commit_type: Some(commit_type.into()),
            scope,
            breaking,
            errors,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_feat_commit() {
        let v = CommitValidator::validate("feat(api): add endpoint");
        assert!(v.valid);
        assert_eq!(v.commit_type.as_deref(), Some("feat"));
    }

    #[test]
    fn invalid_type() {
        let v = CommitValidator::validate("wrong: something");
        assert!(!v.valid);
    }

    #[test]
    fn breaking_detected() {
        let v = CommitValidator::validate("feat!: breaking change");
        assert!(v.breaking);
    }
}
