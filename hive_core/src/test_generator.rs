//! Generación automática de tests.
//!
//! Genera tests para el código que las obreras crean o modifican.
//! Inspirado en Devin que genera tests automáticamente.

use serde::{Deserialize, Serialize};

/// Test generado.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedTest {
    pub function_name: String,
    pub test_name: String,
    pub test_code: String,
    pub test_file: String,
    pub test_type: TestType,
}

/// Tipo de test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestType {
    Unit,
    Integration,
    EdgeCase,
    ErrorCase,
}

/// Generador de tests.
pub struct TestGenerator;

impl TestGenerator {
    /// Genera tests para una función Rust.
    pub fn generate_rust_tests(
        fn_name: &str,
        params: &[(String, String)],
        return_type: &str,
    ) -> Vec<GeneratedTest> {
        let mut tests = Vec::new();

        // Test básico
        let param_calls: Vec<String> = params
            .iter()
            .map(|(_, ty)| default_value_for_type(ty))
            .collect();
        let test_code = format!(
            "#[test]\nfn test_{fn_name}_basic() {{\n    let result = {fn_name}({});\n    // TODO: assert result\n    let _ = result;\n}}",
            param_calls.join(", ")
        );
        tests.push(GeneratedTest {
            function_name: fn_name.to_string(),
            test_name: format!("test_{fn_name}_basic"),
            test_code,
            test_file: format!("tests/{fn_name}_test.rs"),
            test_type: TestType::Unit,
        });

        // Test de edge case
        tests.push(GeneratedTest {
            function_name: fn_name.to_string(),
            test_name: format!("test_{fn_name}_edge_cases"),
            test_code: format!(
                "#[test]\nfn test_{fn_name}_edge_cases() {{\n    // Test con valores límite\n    // TODO: implementar\n}}"
            ),
            test_file: format!("tests/{fn_name}_test.rs"),
            test_type: TestType::EdgeCase,
        });

        tests
    }

    /// Genera tests para una función Python.
    pub fn generate_python_tests(fn_name: &str, params: &[String]) -> Vec<GeneratedTest> {
        let param_calls: Vec<String> = params.iter().map(|_| "None".into()).collect();
        let test_code = format!(
            "def test_{fn_name}_basic():\n    result = {fn_name}({})\n    assert result is not None\n",
            param_calls.join(", ")
        );
        vec![GeneratedTest {
            function_name: fn_name.to_string(),
            test_name: format!("test_{fn_name}_basic"),
            test_code,
            test_file: "tests/test_basic.py".into(),
            test_type: TestType::Unit,
        }]
    }

    /// Genera tests para una función JavaScript.
    pub fn generate_js_tests(fn_name: &str) -> Vec<GeneratedTest> {
        let test_code = format!(
            "test('{fn_name} works', () => {{\n  const result = {fn_name}();\n  expect(result).toBeDefined();\n}});"
        );
        vec![GeneratedTest {
            function_name: fn_name.to_string(),
            test_name: format!("test_{fn_name}"),
            test_code,
            test_file: format!("{fn_name}.test.js"),
            test_type: TestType::Unit,
        }]
    }

    /// Genera tests de error para una función.
    pub fn generate_error_tests(fn_name: &str, lang: &str) -> Vec<GeneratedTest> {
        let test_code = match lang {
            "rust" => format!(
                "#[test]\n#[should_panic]\nfn test_{fn_name}_panics_on_invalid() {{\n    // TODO: pasar input inválido\n}}"
            ),
            "python" => format!(
                "import pytest\n\ndef test_{fn_name}_raises():\n    with pytest.raises(Exception):\n        {fn_name}(None)\n"
            ),
            _ => format!("// Test de error para {fn_name}"),
        };
        vec![GeneratedTest {
            function_name: fn_name.to_string(),
            test_name: format!("test_{fn_name}_error"),
            test_code,
            test_file: format!("tests/{fn_name}_error_test"),
            test_type: TestType::ErrorCase,
        }]
    }
}

fn default_value_for_type(ty: &str) -> String {
    match ty {
        "i32" | "i64" | "u32" | "u64" | "usize" => "0".into(),
        "f32" | "f64" => "0.0".into(),
        "bool" => "false".into(),
        "String" | "&str" => "\"\"".into(),
        s if s.starts_with("Vec") => "vec![]".into(),
        s if s.starts_with("Option") => "None".into(),
        _ => "Default::default()".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_rust_tests_creates_unit() {
        let tests = TestGenerator::generate_rust_tests(
            "add",
            &[("a".into(), "i32".into()), ("b".into(), "i32".into())],
            "i32",
        );
        assert!(tests.len() >= 2);
        assert!(tests[0].test_code.contains("add(0, 0)"));
    }

    #[test]
    fn generate_python_tests_creates() {
        let tests = TestGenerator::generate_python_tests("process", &["data".into()]);
        assert!(!tests.is_empty());
        assert!(tests[0].test_code.contains("def test_process_basic"));
    }

    #[test]
    fn generate_js_tests_creates() {
        let tests = TestGenerator::generate_js_tests("calculate");
        assert!(!tests.is_empty());
        assert!(tests[0].test_code.contains("calculate()"));
    }

    #[test]
    fn generate_error_tests_creates() {
        let tests = TestGenerator::generate_error_tests("parse", "rust");
        assert!(!tests.is_empty());
        assert!(tests[0].test_code.contains("should_panic"));
    }
}
