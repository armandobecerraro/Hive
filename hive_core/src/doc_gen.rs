//! Generación automática de documentación.
//! Genera docstrings, ejemplos, y docs de API.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedDoc {
    pub file: String,
    pub content: String,
    pub doc_type: DocType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DocType {
    ApiReference,
    Readme,
    Docstring,
    Example,
    Tutorial,
}

pub struct DocGenerator;

impl DocGenerator {
    pub fn generate_readme(project_name: &str, description: &str, stack: &str) -> GeneratedDoc {
        let content = format!(
            "# {project_name}\n\n{description}\n\n## Stack\n{stack}\n\n## Getting Started\n\n```bash\ncargo run\n```\n\n## License\nMIT\n"
        );
        GeneratedDoc {
            file: "README.md".into(),
            content,
            doc_type: DocType::Readme,
        }
    }

    pub fn generate_docstring_rust(fn_name: &str, params: &[String], returns: &str) -> String {
        let param_docs: Vec<String> = params
            .iter()
            .map(|p| format!("/// - `{p}`: Parámetro"))
            .collect();
        format!(
            "/// {}\n///\n/// # Returns\n/// {returns}\n",
            param_docs.join("\n/// ")
        )
    }

    pub fn generate_example_rust(fn_name: &str) -> String {
        format!("/// # Example\n/// ```\n/// let result = {fn_name}();\n/// assert!(result.is_ok());\n/// ```\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_readme_works() {
        let doc = DocGenerator::generate_readme("myapp", "A cool app", "Rust");
        assert!(doc.content.contains("myapp"));
        assert!(doc.content.contains("Rust"));
    }
}
