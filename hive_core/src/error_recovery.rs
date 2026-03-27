//! Error recovery avanzado.
//!
//! Parsea errores de compilación (cargo, python, node) y genera fixes dirigidos.
//! Inspirado en Devin y SWE-agent que entienden mensajes de error y proponen soluciones.

use serde::{Deserialize, Serialize};

/// Error parseado.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedError {
    pub error_type: ErrorType,
    pub file_path: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
    pub suggestion: Option<String>,
}

/// Tipo de error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorType {
    Compilation,
    Test,
    Runtime,
    TypeMismatch,
    NotFound,
    Permission,
    Timeout,
    Unknown,
}

/// Parser de errores.
pub struct ErrorParser;

impl ErrorParser {
    /// Parsea errores de cargo/rustc.
    pub fn parse_rust_errors(output: &str) -> Vec<ParsedError> {
        let mut errors = Vec::new();
        for line in output.lines() {
            if line.starts_with("error") || line.contains("-->") {
                let file = extract_file_path(line);
                let (ln, col) = extract_line_col(line);
                let suggestion = extract_suggestion(line, output);
                errors.push(ParsedError {
                    error_type: if line.contains("type") {
                        ErrorType::TypeMismatch
                    } else {
                        ErrorType::Compilation
                    },
                    file_path: file,
                    line: ln,
                    column: col,
                    message: line.to_string(),
                    suggestion,
                });
            }
        }
        errors
    }

    /// Parsea errores de Python.
    pub fn parse_python_errors(output: &str) -> Vec<ParsedError> {
        let mut errors = Vec::new();
        for line in output.lines() {
            if line.contains("Error") || line.contains("Exception") {
                let file = extract_file_path(line);
                let (ln, _) = extract_line_col(line);
                errors.push(ParsedError {
                    error_type: ErrorType::Runtime,
                    file_path: file,
                    line: ln,
                    column: None,
                    message: line.to_string(),
                    suggestion: None,
                });
            }
        }
        errors
    }

    /// Parsea errores de Node.js.
    pub fn parse_node_errors(output: &str) -> Vec<ParsedError> {
        let mut errors = Vec::new();
        for line in output.lines() {
            if line.contains("Error") || line.contains("SyntaxError") || line.contains("TypeError")
            {
                errors.push(ParsedError {
                    error_type: if line.contains("Syntax") {
                        ErrorType::Compilation
                    } else {
                        ErrorType::Runtime
                    },
                    file_path: extract_file_path(line),
                    line: extract_line_col(line).0,
                    column: extract_line_col(line).1,
                    message: line.to_string(),
                    suggestion: None,
                });
            }
        }
        errors
    }

    /// Genera un fix dirigido basado en el error.
    pub fn suggest_fix(error: &ParsedError) -> String {
        match &error.error_type {
            ErrorType::NotFound => {
                format!(
                    "Importar o crear '{}' en {}",
                    error.message,
                    error.file_path.as_deref().unwrap_or("el archivo")
                )
            }
            ErrorType::TypeMismatch => {
                format!(
                    "Verificar los tipos en {} línea {}",
                    error.file_path.as_deref().unwrap_or("?"),
                    error.line.unwrap_or(0)
                )
            }
            ErrorType::Compilation => {
                if let Some(ref sug) = error.suggestion {
                    sug.clone()
                } else {
                    format!(
                        "Revisar el código en {} línea {}",
                        error.file_path.as_deref().unwrap_or("?"),
                        error.line.unwrap_or(0)
                    )
                }
            }
            _ => format!("Investigar: {}", error.message),
        }
    }
}

fn extract_file_path(line: &str) -> Option<String> {
    let parts: Vec<&str> = line.split(|c| c == ':' || c == ' ').collect();
    for part in &parts {
        if part.contains('.') && (part.contains('/') || part.contains('\\')) {
            return Some(part.to_string());
        }
    }
    None
}

fn extract_line_col(line: &str) -> (Option<usize>, Option<usize>) {
    let parts: Vec<&str> = line.split(':').collect();
    let ln = parts.iter().find_map(|p| p.trim().parse::<usize>().ok());
    let col = if parts.len() > 2 {
        parts[2].trim().parse::<usize>().ok()
    } else {
        None
    };
    (ln, col)
}

fn extract_suggestion(_error_line: &str, output: &str) -> Option<String> {
    for line in output.lines() {
        if line.contains("help:") || line.contains("suggestion:") {
            return Some(line.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rust_errors_detects() {
        let output = "error[E0308]: mismatched types\n  --> src/main.rs:10:5\n";
        let errors = ErrorParser::parse_rust_errors(output);
        assert!(!errors.is_empty());
    }

    #[test]
    fn parse_python_errors_detects() {
        let output = "File \"main.py\", line 5, in <module>\nTypeError: unsupported";
        let errors = ErrorParser::parse_python_errors(output);
        assert!(!errors.is_empty());
    }

    #[test]
    fn suggest_fix_returns_suggestion() {
        let error = ParsedError {
            error_type: ErrorType::NotFound,
            file_path: Some("main.rs".into()),
            line: Some(10),
            column: None,
            message: "not found".into(),
            suggestion: None,
        };
        let fix = ErrorParser::suggest_fix(&error);
        assert!(!fix.is_empty());
    }
}
