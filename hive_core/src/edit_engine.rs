//! Motor de edición basado en diffs (edit-based coding).
//!
//! En lugar de reescribir archivos completos, genera patches incrementales.
//! Inspirado en Cursor, Aider y Continue que aplican edits granulares.

use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};

/// Tipo de edición.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EditType {
    Insert {
        line: usize,
        content: String,
    },
    Delete {
        start_line: usize,
        end_line: usize,
    },
    Replace {
        start_line: usize,
        end_line: usize,
        content: String,
    },
    Diff {
        old: String,
        new: String,
    },
}

/// Una edición individual.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEdit {
    pub file_path: String,
    pub edit_type: EditType,
    pub description: String,
    pub applied: bool,
}

/// Resultado de aplicar un edit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditResult {
    pub file_path: String,
    pub success: bool,
    pub lines_added: usize,
    pub lines_removed: usize,
    pub error: Option<String>,
}

/// Generador de diffs entre contenido original y nuevo.
pub struct EditEngine;

impl EditEngine {
    /// Genera un diff unificado entre dos textos.
    pub fn generate_diff(old: &str, new: &str) -> String {
        let diff = TextDiff::from_lines(old, new);
        let mut result = String::new();
        for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
            result.push_str(&format!("{}\n", hunk.header()));
            for change in hunk.iter_changes() {
                let sign = match change.tag() {
                    ChangeTag::Delete => "-",
                    ChangeTag::Insert => "+",
                    ChangeTag::Equal => " ",
                };
                result.push_str(&format!("{sign}{change}"));
            }
        }
        result
    }

    /// Calcula métricas de un diff.
    pub fn diff_stats(old: &str, new: &str) -> DiffStats {
        let diff = TextDiff::from_lines(old, new);
        let mut added = 0usize;
        let mut removed = 0usize;
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Insert => added += 1,
                ChangeTag::Delete => removed += 1,
                ChangeTag::Equal => {}
            }
        }
        DiffStats { added, removed }
    }

    /// Aplica un edit de reemplazo de líneas a un contenido.
    pub fn apply_replace(
        content: &str,
        start: usize,
        end: usize,
        replacement: &str,
    ) -> Result<String, String> {
        let lines: Vec<&str> = content.lines().collect();
        if start == 0 || end > lines.len() || start > end {
            return Err(format!(
                "rango inválido: {start}-{end} de {} líneas",
                lines.len()
            ));
        }
        let mut result = Vec::new();
        result.extend_from_slice(&lines[..start - 1]);
        result.push(replacement);
        if end < lines.len() {
            result.extend_from_slice(&lines[end..]);
        }
        Ok(result.join("\n"))
    }

    /// Aplica un edit de inserción.
    pub fn apply_insert(content: &str, line: usize, insertion: &str) -> Result<String, String> {
        let lines: Vec<&str> = content.lines().collect();
        if line > lines.len() {
            return Err(format!(
                "línea {line} fuera de rango (total: {})",
                lines.len()
            ));
        }
        let mut result = Vec::new();
        result.extend_from_slice(&lines[..line]);
        result.push(insertion);
        result.extend_from_slice(&lines[line..]);
        Ok(result.join("\n"))
    }

    /// Aplica un diff unificado a contenido.
    pub fn apply_unified_diff(content: &str, diff: &str) -> Result<String, String> {
        // Simplificación: para producción usaría una librería de patch apply
        let mut result = content.to_string();
        for hunk_line in diff.lines() {
            if hunk_line.starts_with('-') {
                let line_content = &hunk_line[1..];
                result = result.replacen(line_content, "", 1);
            } else if hunk_line.starts_with('+') {
                result.push_str(&hunk_line[1..]);
                result.push('\n');
            }
        }
        Ok(result)
    }

    /// Convierte un edit a un FileEdit estructurado.
    pub fn create_edit(file_path: &str, edit_type: EditType, description: &str) -> FileEdit {
        FileEdit {
            file_path: file_path.to_string(),
            edit_type,
            description: description.to_string(),
            applied: false,
        }
    }
}

/// Estadísticas de un diff.
#[derive(Debug, Clone, Serialize)]
pub struct DiffStats {
    pub added: usize,
    pub removed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_diff_basic() {
        let diff = EditEngine::generate_diff("hello\nworld\n", "hello\nearth\n");
        assert!(diff.contains("-world"));
        assert!(diff.contains("+earth"));
    }

    #[test]
    fn diff_stats_counts() {
        let stats = EditEngine::diff_stats("a\nb\nc\n", "a\nd\nc\ne\n");
        assert_eq!(stats.removed, 1); // b
        assert_eq!(stats.added, 2); // d, e
    }

    #[test]
    fn apply_replace_basic() {
        let content = "line1\nline2\nline3\n";
        let result = EditEngine::apply_replace(content, 2, 2, "replaced").unwrap();
        assert_eq!(result, "line1\nreplaced\nline3");
    }

    #[test]
    fn apply_insert_basic() {
        let content = "line1\nline3\n";
        let result = EditEngine::apply_insert(content, 1, "line2").unwrap();
        assert_eq!(result, "line1\nline2\nline3");
    }

    #[test]
    fn apply_replace_invalid_range() {
        let content = "a\nb\n";
        assert!(EditEngine::apply_replace(content, 0, 1, "x").is_err());
        assert!(EditEngine::apply_replace(content, 1, 5, "x").is_err());
    }
}
