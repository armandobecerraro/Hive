//! # The Brain - LLM-Powered Code Generator
//! 
//! El Cerebro es el componente central que:
//! - Analiza requerimientos y crea tareas específicas
//! - Genera código real usando LLM
//! - Publica tareas al Blackboard para que los Workers las ejecuten
//! 
//! NO envía órdenes directas a workers - solo publica tareas.

use crate::blackboard::{
    Blackboard, FileAction, FileChange, Specialist, Task, TaskResult, TaskStatus,
};
use crate::discovery::ColonizationReport;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use uuid::Uuid;

/// Result type for async LLM operations
pub type LLMFuture = Pin<Box<dyn std::future::Future<Output = Result<String>> + Send>>;

fn trunc_body(s: &str, max_chars: usize) -> String {
    let mut t: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        t.push('…');
    }
    t
}

/// Cliente LLM - soporta Ollama, OpenAI, Anthropic
pub trait LLMClient: Send + Sync {
    fn complete(&self, prompt: &str) -> LLMFuture;
}

/// Mock LLM client for testing without external dependencies
pub struct MockLLMClient {
    response: String,
}

impl MockLLMClient {
    pub fn new(response: String) -> Self {
        Self { response }
    }
}

impl LLMClient for MockLLMClient {
    fn complete(&self, _prompt: &str) -> LLMFuture {
        let response = self.response.clone();
        Box::pin(async move { Ok(response) })
    }
}

/// Cliente Ollama (local)
pub struct OllamaClient {
    base_url: String,
    model: String,
}

impl OllamaClient {
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            model: model.to_string(),
        }
    }
}

impl LLMClient for OllamaClient {
    fn complete(&self, prompt: &str) -> LLMFuture {
        let client = reqwest::Client::new();
        let url = format!("{}/api/generate", self.base_url);
        let model = self.model.clone();
        let prompt = prompt.to_string();
        
        Box::pin(async move {
            let http = client
                .post(&url)
                .json(&serde_json::json!({
                    "model": model,
                    "prompt": prompt,
                    "stream": false
                }))
                .send()
                .await?;

            let status = http.status();
            let body_text = http.text().await.unwrap_or_default();
            let body: serde_json::Value = serde_json::from_str(&body_text)
                .map_err(|e| anyhow!("Ollama respuesta no-JSON (HTTP {status}): {e}; cuerpo: {}", trunc_body(&body_text, 300)))?;

            if let Some(err) = body.get("error").and_then(|v| v.as_str()) {
                anyhow::bail!("Ollama API: {err} (modelo `{model}`, HTTP {status})");
            }

            if !status.is_success() {
                anyhow::bail!(
                    "Ollama HTTP {status}: {}",
                    trunc_body(&body_text, 500)
                );
            }

            match body.get("response").and_then(|v| v.as_str()) {
                Some(s) if !s.is_empty() => Ok(s.to_string()),
                _ => Err(anyhow!(
                    "Ollama sin `response` útil (HTTP {status}). ¿Modelo instalado? (`ollama pull {}`). Fragmento: {}",
                    model,
                    trunc_body(&body_text, 400)
                )),
            }
        })
    }
}

/// El Cerebro - Genera código real con LLM
pub struct Brain {
    llm_client: Box<dyn LLMClient>,
    blackboard: std::sync::Arc<Blackboard>,
    specialist_type: Specialist,
}

impl Brain {
    pub fn new(
        llm_client: Box<dyn LLMClient>,
        blackboard: std::sync::Arc<Blackboard>,
        specialist_type: Specialist,
    ) -> Self {
        Self {
            llm_client,
            blackboard,
            specialist_type,
        }
    }
    
    /// Analiza el repositorio y genera tareas concretas
    pub async fn analyze_and_generate_tasks(
        &self,
        report: &ColonizationReport,
        target_dir: &PathBuf,
    ) -> Result<Vec<Task>> {
        // Build analysis prompt
        let prompt = self.build_analysis_prompt(report, target_dir);
        
        // Call LLM to get task suggestions
        let response = self.llm_client.complete(&prompt).await?;
        
        // Parse response into tasks
        let tasks = self.parse_tasks_from_response(&response, report)?;
        
        // Publish tasks to blackboard
        for task in &tasks {
            self.blackboard.add_task(task.clone()).await?;
        }
        
        tracing::info!(
            task_count = tasks.len(),
            specialist = ?self.specialist_type,
            "Brain generated tasks"
        );
        
        Ok(tasks)
    }
    
    /// Genera código real para una tarea específica
    pub async fn generate_code_for_task(
        &self,
        task: &Task,
        context: &[crate::blackboard::TaskResult],
    ) -> Result<String> {
        let prompt = self.build_code_prompt(task, context);
        let code = self.llm_client.complete(&prompt).await?;
        
        tracing::debug!(
            task_id = %task.id,
            code_length = code.len(),
            "Brain generated code"
        );
        
        Ok(code)
    }

    /// Una sola llamada al LLM y lista de cambios de archivo (mismo formato que [`parse_llm_code_response`]).
    pub async fn generate_file_changes_for_task(
        &self,
        task: &Task,
        context: &[TaskResult],
    ) -> Result<Vec<FileChange>> {
        let raw = self.generate_code_for_task(task, context).await?;
        Ok(parse_llm_code_response(&raw, &task.target_file))
    }
    
    fn build_analysis_prompt(&self, report: &ColonizationReport, target_dir: &PathBuf) -> String {
        let dna = &report.dna;
        let mut prompt = format!(
            "Eres el Cerebro de un sistema multi-agente de desarrollo.\n\n\
            REPOSITORIO: {}\n\n\
            ADN TÉCNICO:\n\
            - Lenguajes detectados: {:?}\n\
            - Frameworks/Manifiestos: {:?}\n\
            - Deuda técnica: {:?}\n\n",
            target_dir.display(),
            dna.dominant_languages,
            dna.manifest_hits,
            dna.debt
        );
        
        prompt += "BASED ON THE ABOVE ANALYSIS, generate 3-5 SPECIFIC, EXECUTABLE TASKS.\n\n";
        prompt += "FORMAT: Each task must be:\n";
        prompt += "1. File to create/modify (e.g., src/main.rs)\n";
        prompt += "2. Description of what to implement\n";
        prompt += "3. Specialist type: rust, python, test, docs\n\n";
        prompt += "Example:\n";
        prompt += "FILE: src/api.rs\n";
        prompt += "TASK: Create REST API endpoint for user authentication with JWT tokens\n";
        prompt += "SPECIALIST: rust\n\n";
        prompt += "Generate tasks that would improve this codebase:\n";
        
        prompt
    }
    
    fn build_code_prompt(&self, task: &Task, context: &[TaskResult]) -> String {
        let mut prompt = format!(
            "Eres un programador {} experto.\n\n\
            TAREA: {}\n\n\
            ARCHIVO: {}\n\n\
            ESPECIALISTA: {:?}\n\n",
            self.specialist_type,
            task.description,
            task.target_file.display(),
            task.specialist_type
        );
        
        if !context.is_empty() {
            prompt += "CONTEXTO (tareas relacionadas ya completadas):\n";
            for ctx in context.iter().take(5) {
                prompt += &format!("- {}: {:?}\n", ctx.task_id, ctx.changes);
            }
            prompt += "\n";
        }
        
        prompt += "INSTRUCCIONES:\n";
        prompt += "1. Escribe el código COMPLETO y REAL para el archivo especificado\n";
        prompt += "2. El código debe ser funcional, compilable, idiomático\n";
        prompt += "3. NO escribas markdown, NO объяснения, SOLO código\n";
        prompt += "4. El archivo puede ser CREAR (new) o MODIFICAR (modify)\n\n";
        prompt += "RESPONSE FORMAT:\n";
        prompt += "[FILE: ruta/relativa/al/archivo.ext]\n";
        prompt += "[ACTION: create|modify]\n";
        prompt += "[CODE]\n";
        prompt += "<código aquí>\n";
        prompt += "[/CODE]\n\n";
        prompt += "Genera el código:\n";
        
        prompt
    }
    
    fn parse_tasks_from_response(
        &self,
        response: &str,
        report: &ColonizationReport,
    ) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();
        
        // Simple parser - looks for FILE: and TASK: patterns
        let mut current_file = String::new();
        let mut current_task = String::new();
        let mut current_specialist = self.specialist_type;
        
        for line in response.lines() {
            let line = line.trim();
            if line.starts_to_uppercase("FILE:") {
                current_file = line.replace("FILE:", "").trim().to_string();
            } else if line.starts_to_uppercase("TASK:") {
                current_task = line.replace("TASK:", "").trim().to_string();
            } else if line.starts_to_uppercase("SPECIALIST:") {
                let spec_str = line.replace("SPECIALIST:", "").trim().to_lowercase();
                current_specialist = match spec_str.as_str() {
                    "rust" => Specialist::Rust,
                    "python" => Specialist::Python,
                    "test" => Specialist::Test,
                    "docs" => Specialist::Docs,
                    _ => self.specialist_type,
                };
            }
            
            // If we have enough info, create a task
            if !current_file.is_empty() && !current_task.is_empty() {
                tasks.push(Task {
                    id: Uuid::new_v4(),
                    description: current_task.clone(),
                    target_file: PathBuf::from(&current_file),
                    specialist_type: current_specialist,
                    priority: 1,
                    dependencies: vec![],
                    created_by: Uuid::nil(), // Brain created this
                    status: TaskStatus::Pending,
                });
                current_file.clear();
                current_task.clear();
            }
        }
        
        // If no tasks parsed, create a default task based on DNA
        if tasks.is_empty() {
            let default_task = self.create_default_task(report)?;
            tasks.push(default_task);
        }
        
        Ok(tasks)
    }
    
    fn create_default_task(&self, report: &ColonizationReport) -> Result<Task> {
        let (description, target_file) = match report.dna.dominant_languages.first().map(|s| s.as_str()) {
            Some("Rust") | Some("C") | Some("C++") => (
                "Add error handling and logging to main module".to_string(),
                PathBuf::from("src/main.rs"),
            ),
            Some("Python") => (
                "Add type hints and docstrings to main module".to_string(),
                PathBuf::from("main.py"),
            ),
            Some("JavaScript") | Some("TypeScript") => (
                "Add JSDoc comments and error handling".to_string(),
                PathBuf::from("src/index.js"),
            ),
            _ => (
                "Create comprehensive README with setup instructions".to_string(),
                PathBuf::from("README.md"),
            ),
        };
        
        Ok(Task {
            id: Uuid::new_v4(),
            description,
            target_file,
            specialist_type: self.specialist_type,
            priority: 1,
            dependencies: vec![],
            created_by: Uuid::nil(),
            status: TaskStatus::Pending,
        })
    }
}

/// Rutas que el LLM no debe poder escribir (placeholders, traversal, `.git`, absolutas).
fn is_safe_llm_repo_path(rel: &str) -> bool {
    let rel = rel.trim();
    if rel.is_empty()
        || rel.contains('\0')
        || rel.contains('<')
        || rel.contains('>')
        || rel.contains('[')
        || rel.contains(']')
    {
        return false;
    }
    let p = Path::new(rel);
    if p.is_absolute() {
        return false;
    }
    for c in p.components() {
        use std::path::Component;
        match c {
            Component::ParentDir => return false,
            Component::Normal(os) => {
                if os
                    .to_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case(".git"))
                {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

/// Evita que modelos pequeños (p. ej. tinyllama) inunden `.rs`/`.md` con prosa del prompt.
fn content_plausible_for_fallback_target(rel: &Path, content: &str) -> bool {
    let ext = rel
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let t = content.trim();
    if t.is_empty() {
        return false;
    }
    match ext.as_str() {
        "rs" => {
            t.contains("fn ")
                || t.contains("use ")
                || t.contains("struct ")
                || t.contains("enum ")
                || t.contains("impl ")
                || t.contains("trait ")
                || t.contains("type ")
                || t.contains("#![")
                || (t.contains("#[") && (t.contains("derive") || t.contains("cfg(")))
                || (t.contains("mod ") && t.contains(';'))
        }
        "py" => {
            t.contains("def ")
                || t.contains("import ")
                || t.contains("class ")
                || t.contains("from ")
        }
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" => {
            t.contains("function ")
                || t.contains("export ")
                || t.contains("import ")
                || t.contains("const ")
                || t.contains("let ")
        }
        "md" => t.starts_with('#') || t.contains("\n# ") || t.contains("\n## "),
        "toml" => t.contains('[') && t.contains(']'),
        _ => true,
    }
}

/// Interpreta la salida del LLM con bloques `[FILE:…]`, `[ACTION:…]`, `[CODE]…[/CODE]`.
/// Si no hay bloques reconocibles, solo usa `fallback_target` si el texto parece código real
/// para esa extensión (no prosa / instrucciones).
pub fn parse_llm_code_response(response: &str, fallback_target: &Path) -> Vec<FileChange> {
    let mut changes = Vec::new();
    let mut current_file: Option<String> = None;
    let mut current_action = FileAction::Create;
    let mut current_content = String::new();
    let mut in_code_block = false;
    // Si el modelo puso `[FILE: …]` con ruta inválida (p. ej. `<path>`), no volcar toda la respuesta al fallback.
    let mut skip_fallback_after_bad_file = false;

    for line in response.lines() {
        let line_upper = line.to_uppercase();

        if line_upper.starts_with("[FILE:")
            && (line_upper.ends_with(']') || line_upper.contains("] [ACTION"))
        {
            if let Some(file) = current_file.take() {
                if !current_content.trim().is_empty() && is_safe_llm_repo_path(&file) {
                    changes.push(FileChange {
                        path: PathBuf::from(&file),
                        action: current_action.clone(),
                        content: current_content.trim().to_string(),
                        diff: None,
                    });
                }
            }
            let file = line
                .replace("[FILE:", "")
                .replace(']', "")
                .trim()
                .to_string();
            if is_safe_llm_repo_path(&file) {
                current_file = Some(file);
            } else {
                if !file.is_empty() {
                    skip_fallback_after_bad_file = true;
                }
                current_file = None;
            }
            current_content.clear();
            in_code_block = false;
        } else if line_upper.starts_with("[ACTION:") {
            let action = line
                .replace("[ACTION:", "")
                .replace(']', "")
                .trim()
                .to_lowercase();
            current_action = match action.as_str() {
                "create" | "new" => FileAction::Create,
                "modify" | "update" | "edit" => FileAction::Modify,
                "delete" => FileAction::Delete,
                _ => FileAction::Create,
            };
        } else if line_upper == "[CODE]" || line_upper.starts_with("```") {
            in_code_block = true;
        } else if line_upper == "[/CODE]" || (line_upper == "```" && in_code_block) {
            in_code_block = false;
        } else if in_code_block {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    if let Some(file) = current_file {
        if !current_content.trim().is_empty() && is_safe_llm_repo_path(&file) {
            changes.push(FileChange {
                path: PathBuf::from(&file),
                action: current_action,
                content: current_content.trim().to_string(),
                diff: None,
            });
        }
    }

    if changes.is_empty()
        && !skip_fallback_after_bad_file
        && !response.trim().is_empty()
        && content_plausible_for_fallback_target(fallback_target, response)
    {
        changes.push(FileChange {
            path: fallback_target.to_path_buf(),
            action: FileAction::Create,
            content: response.trim().to_string(),
            diff: None,
        });
    }

    changes
}

// Helper trait for case-insensitive starts with
trait StartsToUppercase {
    fn starts_to_uppercase(&self, prefix: &str) -> bool;
}

impl StartsToUppercase for str {
    fn starts_to_uppercase(&self, prefix: &str) -> bool {
        self.to_uppercase().starts_with(&prefix.to_uppercase())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_llm_code_response;
    use crate::blackboard::FileAction;
    use std::path::Path;

    #[test]
    fn parse_llm_code_response_extrae_bloque_file_code() {
        let raw = "[FILE: src/foo.rs]\n[ACTION: create]\n[CODE]\nfn foo() {}\n[/CODE]\n";
        let v = parse_llm_code_response(raw, Path::new("README.md"));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].path, Path::new("src/foo.rs"));
        assert_eq!(v[0].action, FileAction::Create);
        assert!(v[0].content.contains("fn foo"));
    }

    #[test]
    fn parse_llm_code_response_sin_bloques_usa_fallback() {
        let raw = "// solo código\nfn x() {}\n";
        let v = parse_llm_code_response(raw, Path::new("src/lib.rs"));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].path, Path::new("src/lib.rs"));
    }

    #[test]
    fn parse_llm_code_response_prosa_no_corrompe_rs() {
        let raw = "Sin solicitud en hive.request.json. Instrucciones:\n1. Escribe código\n";
        let v = parse_llm_code_response(raw, Path::new("src/lib.rs"));
        assert!(v.is_empty());
    }

    #[test]
    fn parse_llm_code_response_ignora_ruta_placeholder() {
        let raw = "[FILE: <path>]\n[ACTION: create]\n[CODE]\nfn x() {}\n[/CODE]\n";
        let v = parse_llm_code_response(raw, Path::new("src/lib.rs"));
        assert!(v.is_empty());
    }

    #[test]
    fn parse_llm_code_response_ignora_ruta_con_corchetes() {
        let raw = "[FILE: name[1].txt]\n[ACTION: create]\n[CODE]\nx\n[/CODE]\n";
        let v = parse_llm_code_response(raw, Path::new("README.md"));
        assert!(v.is_empty());
    }
}
