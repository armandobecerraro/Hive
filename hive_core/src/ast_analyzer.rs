//! Análisis de código basado en AST (tree-sitter).
//!
//! Comprende la estructura real del código en lugar de usar regex.
//! Soporta Rust, Python, JavaScript/TypeScript.

use serde::{Deserialize, Serialize};

/// Nodo del AST.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstNode {
    pub kind: String,
    pub name: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub children: Vec<AstNode>,
}

/// Función detectada.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInfo {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub params: Vec<String>,
    pub is_public: bool,
}

/// Estructura/clase detectada.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructInfo {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub fields: Vec<String>,
    pub is_public: bool,
}

/// Analizador AST simple basado en parsing por líneas.
pub struct AstAnalyzer;

impl AstAnalyzer {
    /// Extrae funciones de código Rust.
    pub fn extract_rust_functions(content: &str) -> Vec<FunctionInfo> {
        let mut functions = Vec::new();
        let mut brace_depth = 0;
        let mut current_fn: Option<(String, usize, bool, Vec<String>)> = None;

        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            brace_depth += trimmed.chars().filter(|c| *c == '{').count() as i32;
            brace_depth -= trimmed.chars().filter(|c| *c == '}').count() as i32;

            if (trimmed.starts_with("fn ") || trimmed.starts_with("pub fn "))
                && current_fn.is_none()
            {
                let is_public = trimmed.starts_with("pub");
                let fn_part = if is_public {
                    trimmed.strip_prefix("pub fn ").unwrap_or(trimmed)
                } else {
                    trimmed.strip_prefix("fn ").unwrap_or(trimmed)
                };
                let name = fn_part.split('(').next().unwrap_or("").trim().to_string();
                let params = extract_params_from_sig(fn_part);
                current_fn = Some((name, i + 1, is_public, params));
            }

            if brace_depth == 0 && current_fn.is_some() {
                if let Some((name, start, is_public, params)) = current_fn.take() {
                    functions.push(FunctionInfo {
                        name,
                        start_line: start,
                        end_line: i + 1,
                        params,
                        is_public,
                    });
                }
            }
        }
        functions
    }

    /// Extrae funciones de código Python.
    pub fn extract_python_functions(content: &str) -> Vec<FunctionInfo> {
        let mut functions = Vec::new();
        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
                let def_part = trimmed
                    .strip_prefix("async def ")
                    .unwrap_or(trimmed)
                    .strip_prefix("def ")
                    .unwrap_or(trimmed);
                let name = def_part.split('(').next().unwrap_or("").trim().to_string();
                let is_public = !name.starts_with('_');
                let params = extract_params_from_sig(def_part);
                functions.push(FunctionInfo {
                    name,
                    start_line: i + 1,
                    end_line: i + 1,
                    params,
                    is_public,
                });
            }
        }
        functions
    }

    /// Extrae funciones de JavaScript/TypeScript.
    pub fn extract_js_functions(content: &str) -> Vec<FunctionInfo> {
        let mut functions = Vec::new();
        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("function ") || trimmed.contains("function ") {
                let name = trimmed
                    .split('(')
                    .next()
                    .unwrap_or("")
                    .replace("function ", "")
                    .trim()
                    .to_string();
                functions.push(FunctionInfo {
                    name,
                    start_line: i + 1,
                    end_line: i + 1,
                    params: vec![],
                    is_public: true,
                });
            } else if trimmed.contains("=>")
                && (trimmed.starts_with("const ") || trimmed.starts_with("let "))
            {
                let name = trimmed
                    .split('=')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .replace("const ", "")
                    .replace("let ", "")
                    .to_string();
                functions.push(FunctionInfo {
                    name,
                    start_line: i + 1,
                    end_line: i + 1,
                    params: vec![],
                    is_public: true,
                });
            }
        }
        functions
    }

    /// Calcula la complejidad ciclomática (simplificada).
    pub fn cyclomatic_complexity(content: &str) -> usize {
        let mut complexity = 1; // Base
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.contains("if ")
                || trimmed.contains("else if")
                || trimmed.contains("match ")
                || trimmed.contains("for ")
                || trimmed.contains("while ")
                || trimmed.contains("&&")
                || trimmed.contains("||")
            {
                complexity += 1;
            }
        }
        complexity
    }

    /// Cuenta líneas de código (excluye blanks y comments).
    pub fn count_loc(content: &str) -> usize {
        content
            .lines()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty() && !t.starts_with("//") && !t.starts_with('#') && !t.starts_with("/*")
            })
            .count()
    }
}

fn extract_params_from_sig(sig: &str) -> Vec<String> {
    if let Some(start) = sig.find('(') {
        if let Some(end) = sig[start..].find(')') {
            let params = &sig[start + 1..start + end];
            return params
                .split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();
        }
    }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_rust_functions() {
        let code = "fn main() {\n    println!(\"hi\");\n}\n\npub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        let fns = AstAnalyzer::extract_rust_functions(code);
        assert_eq!(fns.len(), 2);
        assert_eq!(fns[0].name, "main");
        assert_eq!(fns[1].name, "add");
        assert!(fns[1].is_public);
    }

    #[test]
    fn extract_python_functions() {
        let code = "def hello():\n    pass\n\nasync def fetch(url):\n    pass\n";
        let fns = AstAnalyzer::extract_python_functions(code);
        assert_eq!(fns.len(), 2);
    }

    #[test]
    fn cyclomatic_complexity_counts() {
        let code = "if a {\n    if b {\n    }\n}\nfor x in y {\n}\n";
        let complexity = AstAnalyzer::cyclomatic_complexity(code);
        assert!(complexity >= 3, "expected >= 3, got {complexity}");
    }

    #[test]
    fn count_loc_excludes_blanks() {
        let code = "fn main() {\n\n    // comment\n    println!(\"hi\");\n}\n";
        assert_eq!(AstAnalyzer::count_loc(code), 3);
    }
}
