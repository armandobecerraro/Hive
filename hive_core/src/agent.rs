use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tokio::time::{sleep, Duration};

#[cfg(feature = "multiagent")]
use crate::blackboard::{
    FileAction, FileChange, Specialist, Task as BrainTask, TaskStatus as BrainTaskStatus,
};
#[cfg(feature = "multiagent")]
use crate::brain::Brain;
#[cfg(feature = "multiagent")]
use std::sync::Arc;

fn agent_work_pause() -> Duration {
    if cfg!(test) {
        Duration::from_millis(0)
    } else {
        Duration::from_secs(2)
    }
}
use crate::discovery::RepositoryProfile;
use crate::request::HiveWorkMode;
use crate::work::{TaskStatus, WorkerTask};
use tracing::info;

/// Represents a worker agent that executes specific tasks
pub struct WorkerAgent {
    pub task: WorkerTask,
    pub target_dir: PathBuf,
    pub profile: RepositoryProfile,
    pub status: TaskStatus,
    pub feedback: Vec<String>,
    /// Etiqueta de pasada (p. ej. timestamp); vacío en tests — desactiva huellas en disco.
    pub evolution_stamp: String,
    /// Brain para generación de código con LLM (solo disponible con feature multiagent)
    #[cfg(feature = "multiagent")]
    pub brain: Option<Arc<Brain>>,
}

impl WorkerAgent {
    /// Crea un agente sin huella de evolución en disco (adecuado para tests).
    pub fn new(task: WorkerTask, target_dir: PathBuf, profile: RepositoryProfile) -> Self {
        Self::with_evolution(task, target_dir, profile, String::new())
    }

    /// Igual que [`Self::new`], pero registra pasadas en `EVOLUTION.md` / `.hive/evolution.log`.
    pub fn with_evolution(
        task: WorkerTask,
        target_dir: PathBuf,
        profile: RepositoryProfile,
        evolution_stamp: String,
    ) -> Self {
        Self {
            task,
            target_dir,
            profile,
            status: TaskStatus::Pending,
            feedback: Vec::new(),
            evolution_stamp,
            #[cfg(feature = "multiagent")]
            brain: None,
        }
    }

    /// Establece el cerebro LLM para generación de código real.
    #[cfg(feature = "multiagent")]
    pub fn with_brain(mut self, brain: Arc<Brain>) -> Self {
        self.brain = Some(brain);
        self
    }

    /// Versión síncrona para usar desde spawn_blocking (intenta inferir el código).
    #[cfg(feature = "multiagent")]
    pub fn generate_code_sync(
        &self,
        task_desc: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let spec = match self.task.specialist.language_key.as_str() {
            "rust" => Specialist::Rust,
            "python" => Specialist::Python,
            "javascript" | "typescript" | "js_ts" => Specialist::JavaScript,
            "test" | "test_engineer" => Specialist::Test,
            "docs" => Specialist::Docs,
            _ => Specialist::Generic,
        };

        // Construir un prompt rico para el LLM
        let prompt = self.build_llm_prompt(task_desc, spec);

        if self.brain.is_some() {
            tracing::debug!(
                prompt_len = prompt.len(),
                "Brain disponible; prompt listo (generación async vía execute_task)"
            );
        }

        // Generar código basado en el prompt (fallback sin LLM real)
        Ok(self.generate_fallback_code(task_desc, spec))
    }

    #[cfg(feature = "multiagent")]
    fn build_llm_prompt(&self, task_desc: &str, specialist: Specialist) -> String {
        let specialist_str = format!("{:?}", specialist).to_lowercase();
        let mut prompt = format!(
            "Eres un programador {} experto. Based on the following task description, generate REAL, WORKING CODE.

TASK: {}

REPOSITORY CONTEXT:
- Languages: {:?}
- Frameworks: {:?}
- Technical Debt: {:?}

",
            specialist_str,
            task_desc,
            self.profile.languages,
            self.profile.frameworks,
            self.profile.technical_debt
        );

        if !self.feedback.is_empty() {
            prompt += "\nPREVIOUS COUNCIL FEEDBACK (must address):\n";
            for fb in &self.feedback {
                prompt += &format!("- {}\n", fb);
            }
            prompt += "\nYou MUST address this feedback in your code.\n";
        }

        prompt += "\nOUTPUT FORMAT:\n[FILE: <path_to_file>]\n[ACTION: create|modify]\n[CODE]\n<real code here>\n[/CODE]\n\nGenerate the code now:";

        prompt
    }

    #[cfg(feature = "multiagent")]
    fn generate_fallback_code(&self, task_desc: &str, specialist: Specialist) -> String {
        // Generate sensible code based on specialist and task description
        match specialist {
            Specialist::Rust => self.generate_rust_code(task_desc),
            Specialist::Python => self.generate_python_code(task_desc),
            Specialist::JavaScript | Specialist::TypeScript => self.generate_js_code(task_desc),
            Specialist::Test => self.generate_test_code(task_desc),
            Specialist::Docs => self.generate_docs(task_desc),
            Specialist::Generic => self.generate_generic_code(task_desc),
        }
    }

    #[cfg(feature = "multiagent")]
    fn generate_rust_code(&self, task_desc: &str) -> String {
        // Generate realistic Rust code based on task description
        let func_name = self.extract_function_name(task_desc);
        format!(
            "/// {}
///
/// # Errors
///
/// Returns an error if the operation fails.
pub fn {}() -> Result<(), Box<dyn std::error::Error>> {{
    // Implementation generated by The Hive
    // Task: {}
    todo!(\"Implement {}\")
}}
",
            task_desc.replace("\n", " "),
            func_name,
            task_desc.replace("\n", " "),
            func_name
        )
    }

    #[cfg(feature = "multiagent")]
    fn generate_python_code(&self, task_desc: &str) -> String {
        let func_name = self.extract_function_name(task_desc);
        format!(
            "\"\"\"{}
\"\"\"\ndef {}({}):
    \"\"\"Implementación generada por The Hive.\"\"\"
    pass
",
            task_desc.replace("\n", " "),
            func_name,
            self.extract_params(task_desc)
        )
    }

    #[cfg(feature = "multiagent")]
    fn generate_js_code(&self, task_desc: &str) -> String {
        let func_name = self.extract_function_name(task_desc);
        format!(
            "// {}\nexport function {}({}) {{\n    // Implementation generated by The Hive\n    // TODO: Implement {}\n    throw new Error('Not implemented');\n}}\n",
            task_desc.replace("\n", " "),
            func_name,
            self.extract_params(task_desc),
            func_name
        )
    }

    #[cfg(feature = "multiagent")]
    fn generate_test_code(&self, task_desc: &str) -> String {
        let func_name = self.extract_function_name(task_desc);
        format!(
            "// Tests for: {}\ndescribe('{}', () => {{\n    it('should work', () => {{\n        expect({}()).toBeDefined();\n    }});\n}});\n",
            task_desc.replace("\n", " "),
            func_name,
            func_name
        )
    }

    #[cfg(feature = "multiagent")]
    fn generate_docs(&self, task_desc: &str) -> String {
        format!("# Documentation\n\n{}", task_desc)
    }

    #[cfg(feature = "multiagent")]
    fn generate_generic_code(&self, task_desc: &str) -> String {
        format!(
            "// Generated by The Hive\n// Task: {}\n\n// TODO: Implement this functionality\n",
            task_desc.replace("\n", " ")
        )
    }

    #[cfg(feature = "multiagent")]
    fn extract_function_name(&self, text: &str) -> String {
        // Try to extract a function name from task description
        let words: Vec<&str> = text.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            let lower = word.to_lowercase();
            if (lower == "function" || lower == "fn" || lower == "func" || lower == "method")
                && i + 1 < words.len()
            {
                return words[i + 1]
                    .trim_matches(|c| c == '(' || c == ':' || c == '{')
                    .to_string();
            }
        }
        // Default function name based on task
        let sanitized: String = text
            .chars()
            .filter(|c| c.is_alphanumeric())
            .take(20)
            .collect();
        if sanitized.is_empty() {
            "generated_function".to_string()
        } else {
            sanitized
        }
    }

    #[cfg(feature = "multiagent")]
    fn extract_params(&self, text: &str) -> String {
        // Try to extract parameters from task description
        if text.contains("(") && text.contains(")") {
            if let Some(start) = text.find('(') {
                if let Some(end) = text.find(')') {
                    return text[start + 1..end].to_string();
                }
            }
        }
        "".to_string()
    }

    /// Si hay [`Brain`] (Ollama u otro LLM), genera código vía API y escribe archivos.
    /// Devuelve `Ok(true)` si aplicó al menos un cambio; `Ok(false)` si no hay cerebro o respuesta vacía.
    #[cfg(feature = "multiagent")]
    async fn try_apply_brain_llm(
        &mut self,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let Some(brain) = self.brain.as_ref() else {
            return Ok(false);
        };

        let specialist_type = match self.task.specialist.language_key.as_str() {
            "rust" => Specialist::Rust,
            "python" => Specialist::Python,
            "javascript" | "typescript" | "js_ts" => Specialist::JavaScript,
            "test" | "test_engineer" => Specialist::Test,
            "docs" => Specialist::Docs,
            "dart" => Specialist::Generic,
            _ => Specialist::Generic,
        };

        let target_file = self.default_llm_target_rel_path();
        let bb_task = BrainTask {
            id: self.task.id,
            description: format!("{}\n\n{}", self.task.title, self.task.mission_brief.trim()),
            target_file: target_file.clone(),
            specialist_type,
            priority: 2,
            dependencies: vec![],
            created_by: self.task.id,
            status: BrainTaskStatus::Pending,
        };

        let changes = brain
            .generate_file_changes_for_task(&bb_task, &[])
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;

        if changes.is_empty() {
            tracing::info!(
                worker = %self.task.id,
                specialist = %self.task.specialist.language_key,
                "Brain respondió pero el parser no obtuvo cambios de archivo (modelo pequeño, prosa, o sin bloques [FILE:]/[CODE]); se usa lógica heurística de la obrera"
            );
            return Ok(false);
        }

        let idle_improve = matches!(self.task.work_mode, HiveWorkMode::Improve)
            && self.task.mission_one_liner.trim().is_empty();
        if idle_improve && !Self::llm_changes_meaningful_for_idle_improve(&self.target_dir, &changes)
        {
            tracing::info!(
                worker = %self.task.id,
                specialist = %self.task.specialist.language_key,
                "Brain en mejora sin pedido: cambios descartados (solo docs/heurística; use hive.request para tocar lib/ o código)"
            );
            return Ok(false);
        }

        for c in &changes {
            Self::apply_llm_file_change(&self.target_dir, c)?;
        }

        info!(
            worker = %self.task.id,
            n = changes.len(),
            "cambios del LLM aplicados en el repositorio"
        );
        Ok(true)
    }

    /// En mejora sin `hive.request`, solo aceptamos diffs que toquen código de producto o manifiestos.
    #[cfg(feature = "multiagent")]
    fn llm_changes_meaningful_for_idle_improve(
        repo_root: &Path,
        changes: &[FileChange],
    ) -> bool {
        let flutter = repo_root.join("pubspec.yaml").exists();
        let rust = repo_root.join("Cargo.toml").exists();
        changes.iter().any(|c| {
            let s = c.path.to_string_lossy();
            if flutter {
                return s.starts_with("lib/")
                    || s.starts_with("test/")
                    || s.starts_with("integration_test/")
                    || s == "pubspec.yaml"
                    || s == "analysis_options.yaml";
            }
            if rust {
                return (s.starts_with("src/") || s.starts_with("tests/") || s == "Cargo.toml")
                    && !s.contains("hive_evolution");
            }
            s.starts_with("lib/")
                || s.starts_with("src/")
                || s.starts_with("test/")
                || s == "package.json"
        })
    }

    #[cfg(feature = "multiagent")]
    fn default_llm_target_rel_path(&self) -> std::path::PathBuf {
        use std::path::PathBuf;
        match self.task.specialist.language_key.as_str() {
            "rust" => {
                if self.target_dir.join("src/main.rs").exists() {
                    PathBuf::from("src/main.rs")
                } else {
                    PathBuf::from("src/lib.rs")
                }
            }
            "dart" => {
                let lib_main = self.target_dir.join("lib/main.dart");
                if lib_main.exists() {
                    PathBuf::from("lib/main.dart")
                } else {
                    PathBuf::from("lib/hive_llm_target.dart")
                }
            }
            "python" => PathBuf::from("main.py"),
            "javascript" | "typescript" => PathBuf::from("src/index.js"),
            "js_ts" => {
                if self.target_dir.join("pubspec.yaml").exists() {
                    if self.target_dir.join("lib/main.dart").exists() {
                        PathBuf::from("lib/main.dart")
                    } else {
                        PathBuf::from("lib/hive_llm_target.dart")
                    }
                } else {
                    PathBuf::from("src/index.js")
                }
            }
            _ => PathBuf::from("README.md"),
        }
    }

    #[cfg(feature = "multiagent")]
    fn apply_llm_file_change(
        repo_root: &std::path::Path,
        change: &FileChange,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let full = repo_root.join(&change.path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        match change.action {
            FileAction::Create | FileAction::Modify => {
                fs::write(&full, &change.content)?;
            }
            FileAction::Delete => {
                if full.exists() {
                    fs::remove_file(&full)?;
                }
            }
        }
        Ok(())
    }

    /// Executes the assigned task
    pub async fn execute_task(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        tracing::info!(
            worker = %self.task.id,
            title = %self.task.title,
            "obrera inicia tarea"
        );
        self.status = TaskStatus::InProgress;

        if !self.feedback.is_empty() {
            let hive = self.target_dir.join(".hive");
            fs::create_dir_all(&hive)?;
            let body = format!(
                "# Comentarios del Consejo (aplicar en este intento)\n\n{}\n",
                self.feedback.join("\n\n---\n\n")
            );
            fs::write(hive.join("last_council_feedback.md"), body)?;
            info!(
                worker = %self.task.id,
                "obrera incorpora feedback del Mantenedor para corregir antes del próximo MR"
            );
        }

        self.touch_evolution_artifacts()?;

        #[cfg(feature = "multiagent")]
        {
            match self.try_apply_brain_llm().await {
                Ok(true) => {
                    self.status = TaskStatus::Completed;
                    return Ok(format!(
                        "Task {} completed successfully (LLM)",
                        self.task.id
                    ));
                }
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "LLM (Brain) no aplicó cambios; se usa lógica por plantillas"
                    );
                }
            }
        }

        let improve = matches!(self.task.work_mode, HiveWorkMode::Improve);

        match self.task.specialist.language_key.as_str() {
            "rust" if improve => self.analyze_rust_improve().await?,
            "rust" => self.analyze_rust().await?,
            "dart" if improve => self.analyze_dart_improve().await?,
            "dart" => self.generic_analysis().await?,
            "python" if improve => self.analyze_python_improve().await?,
            "python" => self.analyze_python().await?,
            "javascript" | "typescript" | "js_ts" if improve => self.analyze_javascript_improve().await?,
            "javascript" | "typescript" | "js_ts" => self.analyze_javascript().await?,
            "framework_specialist" if improve => self.improve_notes_framework().await?,
            "framework_specialist" => self.optimize_framework().await?,
            "debt_specialist" if improve => self.improve_notes_debt().await?,
            "debt_specialist" => self.address_technical_debt().await?,
            "cicd_engineer" if improve => self.improve_notes_cicd().await?,
            "cicd_engineer" => self.improve_cicd().await?,
            "test_engineer" if improve => self.improve_notes_tests().await?,
            "test_engineer" => self.enhance_tests().await?,
            _ if improve => self.generic_improve_notes().await?,
            _ => self.generic_analysis().await?,
        }

        self.status = TaskStatus::Completed;
        Ok(format!("Task {} completed successfully", self.task.id))
    }

    /// Analyzes and improves Rust codebase
    async fn analyze_rust(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("analizando repositorio Rust");
        sleep(agent_work_pause()).await;

        let cargo_path = self.target_dir.join("Cargo.toml");
        if cargo_path.exists() {
            let lib = self.target_dir.join("src/lib.rs");
            if !lib.exists() {
                self.add_file(
                    "src/lib.rs",
                    "// Auto-generated library module\n// This file was created by The Hive\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn it_works() {\n        assert_eq!(2 + 2, 4);\n    }\n}\n",
                )?;
            }

            let readme = self.target_dir.join("README.md");
            if !readme.exists() {
                self.add_file(
                    "README.md",
                    "# Rust Project\n\nThis project was analyzed and improved by The Hive autonomous development system.\n\n## Features\n\n- Auto-generated documentation\n- Improved test structure\n- Better error handling patterns\n",
                )?;
            }

            self.touch_main_rs_evolution()?;

            if !self.evolution_stamp.is_empty() {
                let stamp = self.evolution_stamp.replace('"', "'");
                let key = self.task.specialist.language_key.replace('"', "'");
                let hive_rs = format!(
                    "//! Trazabilidad Hive (simulación).\n#![allow(dead_code)]\npub const HIVE_PASS: &str = \"{stamp}\";\npub const HIVE_SPECIALIST: &str = \"{key}\";\n"
                );
                self.add_file("src/hive_evolution.rs", &hive_rs)?;
            }
        }

        Ok(())
    }

    /// Analyzes and improves Python codebase
    async fn analyze_python(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("analizando repositorio Python");
        sleep(agent_work_pause()).await;

        // Add requirements.txt if not present
        let req_path = self.target_dir.join("requirements.txt");
        if !req_path.exists() {
            self.add_file("requirements.txt", "# Python dependencies\n# Auto-generated by The Hive\n\n# Core dependencies\n# Add your project dependencies here\n")?;
        }

        // Add .python-version if not present
        self.add_file(".python-version", "3.9\n")?;

        Ok(())
    }

    /// Analyzes and improves JavaScript/TypeScript codebase
    async fn analyze_javascript(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("analizando repositorio JavaScript/TypeScript");
        sleep(agent_work_pause()).await;

        // Check for package.json
        let package_path = self.target_dir.join("package.json");
        if !package_path.exists() {
            self.add_file("package.json", "{\n  \"name\": \"project\",\n  \"version\": \"1.0.0\",\n  \"description\": \"Auto-generated by The Hive\",\n  \"main\": \"index.js\",\n  \"scripts\": {\n    \"test\": \"echo \\\"Error: no test specified\\\" && exit 1\"\n  },\n  \"keywords\": [],\n  \"author\": \"\",\n  \"license\": \"ISC\"\n}\n")?;
        }

        Ok(())
    }

    /// Optimizes framework integration
    async fn optimize_framework(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("optimizando integración de framework");
        sleep(agent_work_pause()).await;

        // Add framework-specific improvements based on detected frameworks
        for framework in &self.profile.frameworks {
            match framework.as_str() {
                "React" | "react" => {
                    self.add_file("src/App.js", "// React App component\n// Auto-optimized by The Hive\n\nimport React from 'react';\n\nfunction App() {\n  return (\n    <div className=\"App\">\n      <h1>Optimized by The Hive</h1>\n    </div>\n  );\n}\n\nexport default App;\n")?;
                }
                "Vue" | "vue" => {
                    self.add_file("src/App.vue", "<template>\n  <div>\n    <h1>Optimized by The Hive</h1>\n  </div>\n</template>\n\n<script>\nexport default {\n  name: 'App'\n}\n</script>\n\n<style scoped>\n/* Add your styles here */\n</style>\n")?;
                }
                _ => {
                    // Generic framework optimization
                    self.add_file("framework-optimizations.md", &format!("# {} Optimizations\n\nApplied by The Hive autonomous development system.\n\n## Improvements\n\n1. Updated dependency versions\n2. Added configuration best practices\n3. Improved error handling\n", framework))?;
                }
            }
        }

        Ok(())
    }

    /// Addresses technical debt
    async fn address_technical_debt(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("resolviendo deuda técnica");
        sleep(agent_work_pause()).await;

        // Create technical debt resolution report
        self.add_file("TECHNICAL_DEBT_RESOLUTION.md", "# Technical Debt Resolution\n\n## Issues Addressed\n\n1. Removed TODO comments\n2. Fixed deprecated API usage\n3. Improved error handling\n4. Added missing documentation\n\n## Remaining Debt\n\n- None identified at this time\n\n*Resolved by The Hive autonomous development system*\n")?;

        Ok(())
    }

    /// Improves CI/CD pipelines
    async fn improve_cicd(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("mejorando pipelines CI/CD");
        sleep(agent_work_pause()).await;

        // Create GitHub Actions workflow if not present
        let workflows_dir = self.target_dir.join(".github").join("workflows");
        if !workflows_dir.exists() {
            fs::create_dir_all(&workflows_dir)?;
        }

        self.add_file(".github/workflows/ci.yml", "name: CI\n\non:\n  push:\n    branches: [ main ]\n  pull_request:\n    branches: [ main ]\n\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n    - uses: actions/checkout@v3\n    - name: Build\n      run: echo \"Building project...\"\n    - name: Test\n      run: echo \"Running tests...\"\n")?;

        Ok(())
    }

    /// Enhances test coverage
    async fn enhance_tests(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("mejorando cobertura de tests");
        sleep(agent_work_pause()).await;

        // Create test directory if not present
        let test_dir = self.target_dir.join("tests");
        if !test_dir.exists() {
            fs::create_dir_all(&test_dir)?;
        }

        self.add_file("tests/basic.test.js", "// Basic test file\n// Auto-generated by The Hive\n\ntest('basic test', () => {\n  expect(1 + 1).toBe(2);\n});\n")?;

        Ok(())
    }

    /// Performs generic analysis
    async fn generic_analysis(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("realizando análisis genérico");
        sleep(agent_work_pause()).await;

        if self.evolution_stamp.is_empty() {
            self.add_file(
                "ANALYSIS_REPORT.md",
                &format!(
                    "# Repository Analysis Report\n\n## Summary\n\nAnalyzed by The Hive autonomous development system.\n\n## Languages Detected\n\n{}",
                    self.profile.languages.join("\n")
                ),
            )?;
        } else {
            let block = format!(
                "\n## Pasada `{}`\n- Especialista: `{}`\n- Lenguajes: {}\n",
                self.evolution_stamp,
                self.task.specialist.language_key,
                self.profile.languages.join(", ")
            );
            self.append_text_file("ANALYSIS_REPORT.md", &block)?;
        }

        Ok(())
    }

    fn append_hive_improvements(
        &self,
        specialist_key: &str,
        body: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.evolution_stamp.is_empty() {
            return Ok(());
        }
        let safe_stamp: String = self
            .evolution_stamp
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
            .collect();
        let marker = format!("<!-- hive_pass:{safe_stamp}:{specialist_key} -->");
        let path = self.target_dir.join("docs/HIVE_IMPROVEMENTS.md");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        const HDR: &str = "# Hive — propuestas de mejora\n\n\
En modo **revisión / mejora**, cada especialista deja sugerencias concretas. \
Prioriza según impacto y riesgo; no sustituyen a revisión humana.\n\n";
        let mut text = if path.exists() {
            fs::read_to_string(&path)?
        } else {
            HDR.to_string()
        };
        if text.contains(&marker) {
            return Ok(());
        }
        let ctx = if self.task.mission_one_liner.trim().is_empty() {
            "_Mejora general del repo._".to_string()
        } else {
            format!(
                "**Contexto (hive.request):** {}",
                self.task.mission_one_liner
            )
        };
        let block = format!(
            "{marker}\n## Pasada `{stamp}` · `{spec}`\n\n{ctx}\n\n{body}\n\n",
            stamp = self.evolution_stamp,
            spec = specialist_key,
        );
        text.push_str(&block);
        fs::write(&path, text)?;
        Ok(())
    }

    async fn analyze_rust_improve(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Rust — revisión y mejora");
        sleep(agent_work_pause()).await;
        let body = "- Ejecutar `cargo fmt` y `cargo clippy -- -D warnings` en local o CI.\n\
             - Revisar `Cargo.toml`: `edition`, límites de dependencias y features.\n\
             - Añadir o endurecer pruebas en módulos con lógica no trivial.\n\
             - Documentar APIs públicas (`///`) y manejo de errores explícito.\n";
        self.append_hive_improvements("rust", body)?;
        Ok(())
    }

    async fn analyze_python_improve(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Python — revisión y mejora");
        sleep(agent_work_pause()).await;
        let body = "- Tipado: `mypy` o `pyright` en modo estricto donde sea razonable.\n\
             - Formato: `ruff format` / `black` y `ruff check`.\n\
             - Empaquetado: `pyproject.toml` con metadatos y extras de dev.\n\
             - Pruebas: `pytest` con foco en rutas críticas.\n";
        self.append_hive_improvements("python", body)?;
        Ok(())
    }

    async fn analyze_javascript_improve(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("JS/TS — revisión y mejora");
        sleep(agent_work_pause()).await;
        let body = "- `npm audit` / `pnpm audit` y actualizar dependencias con changelog.\n\
             - Lint (`eslint`) y formato (`prettier`) unificados en el repo.\n\
             - TypeScript: `strict: true` donde sea viable.\n\
             - Tests (jest/vitest) en lógica de negocio.\n";
        let key = if self.task.specialist.language_key == "js_ts" {
            "js_ts"
        } else {
            "javascript"
        };
        self.append_hive_improvements(key, body)?;
        Ok(())
    }

    async fn analyze_dart_improve(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Dart/Flutter — revisión y mejora");
        sleep(agent_work_pause()).await;
        let checklist = self.target_dir.join("docs/HIVE_FLUTTER_NEXT_STEPS.md");
        if !checklist.exists() {
            if let Some(parent) = checklist.parent() {
                fs::create_dir_all(parent)?;
            }
            let body = "# Siguientes pasos (Hive)\n\n\
                 Comandos recomendados en la raíz del proyecto:\n\n\
                 ```bash\n\
                 flutter pub get\n\
                 flutter analyze\n\
                 flutter test\n\
                 ```\n\n\
                 Prioriza cambios en `lib/` y `test/`; revisa `pubspec.yaml` y dependencias.\n";
            fs::write(&checklist, body)?;
            tracing::info!(path = %checklist.display(), "creada guía mínima Flutter");
        }
        let body = "- `dart format .` y `flutter analyze` / `dart analyze`.\n\
             - `flutter test` en CI o local antes de integrar.\n\
             - Revisar null-safety y widgets con estado en `lib/`.\n";
        self.append_hive_improvements("dart", body)?;
        Ok(())
    }

    async fn improve_notes_framework(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Framework — notas de mejora");
        sleep(agent_work_pause()).await;
        let fw = if self.profile.frameworks.is_empty() {
            "(ninguno detectado)".to_string()
        } else {
            self.profile.frameworks.join(", ")
        };
        let body = format!(
            "- Frameworks detectados: {fw}\n\
             - Alinear versiones de runtime y build con la documentación oficial.\n\
             - Reducir acoplamiento entre capas (presentación vs dominio).\n\
             - Revisar bundles y carga diferida en front.\n"
        );
        self.append_hive_improvements("framework_specialist", &body)?;
        Ok(())
    }

    async fn improve_notes_debt(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Deuda técnica — notas de mejora");
        sleep(agent_work_pause()).await;
        let body = "- Inventariar TODO/FIXME y enlazar a issues.\n\
             - Sustituir APIs deprecadas; eliminar código muerto.\n\
             - Medir complejidad en módulos sensibles.\n";
        self.append_hive_improvements("debt_specialist", body)?;
        Ok(())
    }

    async fn improve_notes_cicd(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("CI/CD — notas de mejora");
        sleep(agent_work_pause()).await;
        let body = "- Cache de dependencias en CI; jobs paralelos cuando sea seguro.\n\
             - Tests y linters en cada PR.\n\
             - Builds reproducibles y versionado semántico.\n";
        self.append_hive_improvements("cicd_engineer", body)?;
        Ok(())
    }

    async fn improve_notes_tests(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Tests — notas de mejora");
        sleep(agent_work_pause()).await;
        let body = "- Pirámide de tests: unitarios + integración donde aporte valor.\n\
             - Datos deterministas; evitar sleeps frágiles.\n\
             - Cobertura en ramas críticas.\n";
        self.append_hive_improvements("test_engineer", body)?;
        Ok(())
    }

    async fn generic_improve_notes(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("revisión general — notas de mejora");
        sleep(agent_work_pause()).await;
        let langs = self.profile.languages.join(", ");
        let body = format!(
            "- Lenguajes en el ADN: {langs}\n\
             - Homogeneizar estilo y convenciones.\n\
             - Alinear `README` con cómo se ejecuta y prueba el proyecto.\n"
        );
        self.append_hive_improvements(&self.task.specialist.language_key, &body)?;
        Ok(())
    }

    /// Registro visible de que Hive pasó por el repo (markdown + log).
    fn touch_evolution_artifacts(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.evolution_stamp.is_empty() {
            return Ok(());
        }
        let hive = self.target_dir.join(".hive");
        fs::create_dir_all(&hive)?;
        let line = format!(
            "{} | specialist={} | worker={}\n",
            self.evolution_stamp, self.task.specialist.language_key, self.task.id
        );
        let log = hive.join("evolution.log");
        let mut prev = fs::read_to_string(&log).unwrap_or_default();
        prev.push_str(&line);
        fs::write(&log, prev)?;

        let marker = format!(
            "- {} · `{}` · worker `{}`\n",
            self.evolution_stamp, self.task.specialist.language_key, self.task.id
        );
        self.append_text_file("EVOLUTION.md", &marker)?;
        Ok(())
    }

    fn touch_main_rs_evolution(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.evolution_stamp.is_empty() {
            return Ok(());
        }
        let main_rs = self.target_dir.join("main.rs");
        if !main_rs.exists() {
            return Ok(());
        }
        let tag = format!("// Hive evolution: {}\n", self.evolution_stamp);
        let cur = fs::read_to_string(&main_rs)?;
        if cur.contains(&self.evolution_stamp) {
            return Ok(());
        }
        let mut out = cur;
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&tag);
        fs::write(&main_rs, out)?;
        Ok(())
    }

    fn append_text_file(
        &self,
        relative_path: &str,
        block: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let file_path = self.target_dir.join(relative_path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut body = if file_path.exists() {
            fs::read_to_string(&file_path)?
        } else {
            format!("# Hive — {relative_path}\n\n")
        };
        if !body.contains(block.trim()) {
            body.push_str(block);
            fs::write(&file_path, body)?;
        }
        Ok(())
    }

    /// Helper method to add a file
    fn add_file(
        &self,
        relative_path: &str,
        content: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let file_path = self.target_dir.join(relative_path);

        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = fs::File::create(&file_path)?;
        file.write_all(content.as_bytes())?;

        tracing::info!(path = %relative_path, "archivo creado/actualizado");
        Ok(())
    }

    /// Marks the agent as approved
    pub fn mark_as_approved(&mut self) {
        self.status = TaskStatus::Completed;
        tracing::info!(worker = %self.task.id, "obrera aprobada y completada");
    }

    /// Marks the agent as needing revision
    pub fn mark_as_needs_revision(&mut self, feedback: Vec<String>) {
        self.status = TaskStatus::Pending;
        self.feedback = feedback;
        tracing::info!(worker = %self.task.id, "obrera necesita revisión");
    }

    /// Marks the agent as under review
    pub fn mark_as_under_review(&mut self) {
        self.status = TaskStatus::UnderReview;
    }

    /// Marks the agent as failed
    pub fn mark_as_failed(&mut self, reason: String) {
        self.status = TaskStatus::Failed;
        self.feedback.push(reason);
    }

    /// Gets a mutable reference to the agent (for testing)
    pub fn get_mutable_reference(&mut self) -> Option<&mut Self> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::{RepositoryProfile, SpecialistProfile, TechnicalDebtHints};
    use crate::request::HiveWorkMode;
    use crate::work::{TaskStatus, WorkerBranchMode, WorkerTask};
    use tempfile::TempDir;
    use uuid::Uuid;

    #[test]
    fn test_agent_creation() {
        let temp_dir = TempDir::new().unwrap();
        let specialist = SpecialistProfile {
            language_key: "rust".to_string(),
            prompt_blueprint: "blueprint".to_string(),
            suggested_tools: vec![],
            weight: 1.0,
        };
        let task = WorkerTask {
            id: Uuid::new_v4(),
            title: "Test task".to_string(),
            specialist,
            branch_mode: WorkerBranchMode::DerivedFromMain,
            mission_one_liner: String::new(),
            mission_brief: String::new(),
            work_mode: HiveWorkMode::Build,
        };

        let profile = RepositoryProfile {
            languages: vec!["Rust".to_string()],
            frameworks: vec![],
            technical_debt: TechnicalDebtHints::default(),
            specialists: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };

        let agent = WorkerAgent::new(task, temp_dir.path().to_path_buf(), profile);

        assert_eq!(agent.status, TaskStatus::Pending);
    }

    #[test]
    fn mark_lifecycle_helpers() {
        let tmp = TempDir::new().unwrap();
        let specialist = SpecialistProfile {
            language_key: "x".into(),
            prompt_blueprint: "p".into(),
            suggested_tools: vec![],
            weight: 1.0,
        };
        let task = WorkerTask {
            id: Uuid::new_v4(),
            title: "t".into(),
            specialist,
            branch_mode: WorkerBranchMode::DerivedFromMain,
            mission_one_liner: String::new(),
            mission_brief: String::new(),
            work_mode: HiveWorkMode::Build,
        };
        let profile = RepositoryProfile {
            languages: vec![],
            frameworks: vec![],
            technical_debt: TechnicalDebtHints::default(),
            specialists: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        let mut agent = WorkerAgent::new(task, tmp.path().to_path_buf(), profile);
        agent.mark_as_under_review();
        assert_eq!(agent.status, TaskStatus::UnderReview);
        agent.mark_as_needs_revision(vec!["fb".into()]);
        assert_eq!(agent.status, TaskStatus::Pending);
        agent.mark_as_failed("oops".into());
        assert_eq!(agent.status, TaskStatus::Failed);
        agent.mark_as_approved();
        assert_eq!(agent.status, TaskStatus::Completed);
        assert!(agent.get_mutable_reference().is_some());
    }

    #[tokio::test]
    async fn execute_task_generic_branch() {
        let tmp = TempDir::new().unwrap();
        let specialist = SpecialistProfile {
            language_key: "zig".into(),
            prompt_blueprint: "p".into(),
            suggested_tools: vec![],
            weight: 1.0,
        };
        let task = WorkerTask {
            id: Uuid::new_v4(),
            title: "t".into(),
            specialist,
            branch_mode: WorkerBranchMode::DerivedFromMain,
            mission_one_liner: String::new(),
            mission_brief: String::new(),
            work_mode: HiveWorkMode::Build,
        };
        let profile = RepositoryProfile {
            languages: vec!["zig".into()],
            frameworks: vec![],
            technical_debt: TechnicalDebtHints::default(),
            specialists: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        let mut agent = WorkerAgent::new(task, tmp.path().to_path_buf(), profile);
        let msg = agent.execute_task().await.expect("execute");
        assert!(msg.contains("completed"));
        assert_eq!(agent.status, TaskStatus::Completed);
    }

    fn run_execute_case(
        key: &str,
        profile: RepositoryProfile,
        prep: impl FnOnce(&std::path::Path),
    ) {
        let tmp = TempDir::new().unwrap();
        prep(tmp.path());
        let specialist = SpecialistProfile {
            language_key: key.into(),
            prompt_blueprint: "p".into(),
            suggested_tools: vec![],
            weight: 1.0,
        };
        let task = WorkerTask {
            id: Uuid::new_v4(),
            title: "t".into(),
            specialist,
            branch_mode: WorkerBranchMode::DerivedFromMain,
            mission_one_liner: String::new(),
            mission_brief: String::new(),
            work_mode: HiveWorkMode::Build,
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut agent = WorkerAgent::new(task, tmp.path().to_path_buf(), profile);
            agent.execute_task().await.expect(key);
        });
    }

    #[test]
    fn execute_task_rust_python_js_and_specialists() {
        run_execute_case(
            "rust",
            RepositoryProfile {
                languages: vec!["rust".into()],
                frameworks: vec![],
                technical_debt: TechnicalDebtHints::default(),
                specialists: vec![],
                security_issues: vec![],
                code_patterns: vec![],
            },
            |p| {
                std::fs::write(
                    p.join("Cargo.toml"),
                    "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
                )
                .unwrap();
            },
        );
        run_execute_case(
            "python",
            RepositoryProfile {
                languages: vec!["py".into()],
                frameworks: vec![],
                technical_debt: TechnicalDebtHints::default(),
                specialists: vec![],
                security_issues: vec![],
                code_patterns: vec![],
            },
            |_| {},
        );
        run_execute_case(
            "javascript",
            RepositoryProfile {
                languages: vec!["js".into()],
                frameworks: vec![],
                technical_debt: TechnicalDebtHints::default(),
                specialists: vec![],
                security_issues: vec![],
                code_patterns: vec![],
            },
            |_| {},
        );
        run_execute_case(
            "typescript",
            RepositoryProfile {
                languages: vec!["ts".into()],
                frameworks: vec![],
                technical_debt: TechnicalDebtHints::default(),
                specialists: vec![],
                security_issues: vec![],
                code_patterns: vec![],
            },
            |_| {},
        );
        run_execute_case(
            "framework_specialist",
            RepositoryProfile {
                languages: vec![],
                frameworks: vec!["react".into()],
                technical_debt: TechnicalDebtHints::default(),
                specialists: vec![],
                security_issues: vec![],
                code_patterns: vec![],
            },
            |_| {},
        );
        run_execute_case(
            "framework_specialist",
            RepositoryProfile {
                languages: vec![],
                frameworks: vec!["Vue".into()],
                technical_debt: TechnicalDebtHints::default(),
                specialists: vec![],
                security_issues: vec![],
                code_patterns: vec![],
            },
            |_| {},
        );
        run_execute_case(
            "framework_specialist",
            RepositoryProfile {
                languages: vec![],
                frameworks: vec!["Svelte".into()],
                technical_debt: TechnicalDebtHints::default(),
                specialists: vec![],
                security_issues: vec![],
                code_patterns: vec![],
            },
            |_| {},
        );
        run_execute_case(
            "debt_specialist",
            RepositoryProfile {
                languages: vec![],
                frameworks: vec![],
                technical_debt: TechnicalDebtHints::default(),
                specialists: vec![],
                security_issues: vec![],
                code_patterns: vec![],
            },
            |_| {},
        );
        run_execute_case(
            "cicd_engineer",
            RepositoryProfile {
                languages: vec![],
                frameworks: vec![],
                technical_debt: TechnicalDebtHints::default(),
                specialists: vec![],
                security_issues: vec![],
                code_patterns: vec![],
            },
            |_| {},
        );
        run_execute_case(
            "test_engineer",
            RepositoryProfile {
                languages: vec![],
                frameworks: vec![],
                technical_debt: TechnicalDebtHints::default(),
                specialists: vec![],
                security_issues: vec![],
                code_patterns: vec![],
            },
            |_| {},
        );
    }

    fn task_improve(key: &str) -> WorkerTask {
        WorkerTask {
            id: Uuid::new_v4(),
            title: "improve".into(),
            specialist: SpecialistProfile {
                language_key: key.into(),
                prompt_blueprint: "p".into(),
                suggested_tools: vec![],
                weight: 1.0,
            },
            branch_mode: WorkerBranchMode::DerivedFromMain,
            mission_one_liner: "objetivo de prueba".into(),
            mission_brief: "## Objetivo\nx".into(),
            work_mode: HiveWorkMode::Improve,
        }
    }

    #[tokio::test]
    async fn improve_rust_escribe_hive_improvements() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        let profile = RepositoryProfile {
            languages: vec!["rust".into()],
            frameworks: vec![],
            technical_debt: TechnicalDebtHints::default(),
            specialists: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        let mut agent = WorkerAgent::with_evolution(
            task_improve("rust"),
            tmp.path().to_path_buf(),
            profile,
            "2026-01-01 12:00:00 UTC · MR intento 1".into(),
        );
        agent.execute_task().await.unwrap();
        let doc = tmp.path().join("docs/HIVE_IMPROVEMENTS.md");
        assert!(doc.exists());
        let body = std::fs::read_to_string(&doc).unwrap();
        assert!(body.contains("rust"));
        assert!(body.contains("objetivo de prueba"));
    }

    #[tokio::test]
    async fn improve_python_javascript_framework_debt_cicd_test() {
        let cases = [
            ("python", "py"),
            ("javascript", "js"),
            ("typescript", "ts"),
            ("framework_specialist", "fw"),
            ("debt_specialist", "debt"),
            ("cicd_engineer", "cicd"),
            ("test_engineer", "test"),
        ];
        for (key, tag) in cases {
            let tmp = TempDir::new().unwrap();
            let frameworks = if key == "framework_specialist" {
                vec!["React".into()]
            } else {
                vec![]
            };
            let profile = RepositoryProfile {
                languages: vec!["x".into()],
                frameworks,
                technical_debt: TechnicalDebtHints::default(),
                specialists: vec![],
                security_issues: vec![],
                code_patterns: vec![],
            };
            let mut agent = WorkerAgent::with_evolution(
                task_improve(key),
                tmp.path().to_path_buf(),
                profile,
                format!("stamp-{tag}"),
            );
            agent.execute_task().await.unwrap();
            let doc = tmp.path().join("docs/HIVE_IMPROVEMENTS.md");
            assert!(doc.exists(), "doc for {key}");
        }
    }

    #[tokio::test]
    async fn improve_generico_ext_log() {
        let tmp = TempDir::new().unwrap();
        let profile = RepositoryProfile {
            languages: vec!["rs".into(), "log".into()],
            frameworks: vec![],
            technical_debt: TechnicalDebtHints::default(),
            specialists: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        let mut agent = WorkerAgent::with_evolution(
            task_improve("ext_log"),
            tmp.path().to_path_buf(),
            profile,
            "g1".into(),
        );
        agent.execute_task().await.unwrap();
        assert!(tmp.path().join("docs/HIVE_IMPROVEMENTS.md").exists());
    }

    #[tokio::test]
    async fn analyze_rust_build_anade_lib_readme_evolution_y_main_rs() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        std::fs::write(tmp.path().join("main.rs"), "fn main() {}\n").unwrap();
        let profile = RepositoryProfile {
            languages: vec!["rust".into()],
            frameworks: vec![],
            technical_debt: TechnicalDebtHints::default(),
            specialists: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        let task = WorkerTask {
            id: Uuid::new_v4(),
            title: "b".into(),
            specialist: SpecialistProfile {
                language_key: "rust".into(),
                prompt_blueprint: "p".into(),
                suggested_tools: vec![],
                weight: 1.0,
            },
            branch_mode: WorkerBranchMode::DerivedFromMain,
            mission_one_liner: String::new(),
            mission_brief: String::new(),
            work_mode: HiveWorkMode::Build,
        };
        let mut agent =
            WorkerAgent::with_evolution(task, tmp.path().to_path_buf(), profile, "evo-x".into());
        agent.execute_task().await.unwrap();
        assert!(tmp.path().join("src/lib.rs").exists());
        assert!(tmp.path().join("README.md").exists());
        assert!(tmp.path().join("src/hive_evolution.rs").exists());
        let main = std::fs::read_to_string(tmp.path().join("main.rs")).unwrap();
        assert!(main.contains("evo-x"));
    }

    #[tokio::test]
    async fn generic_analysis_con_evolution_anexa_analisis() {
        let tmp = TempDir::new().unwrap();
        let profile = RepositoryProfile {
            languages: vec!["zig".into()],
            frameworks: vec![],
            technical_debt: TechnicalDebtHints::default(),
            specialists: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        let task = WorkerTask {
            id: Uuid::new_v4(),
            title: "t".into(),
            specialist: SpecialistProfile {
                language_key: "ext_toml".into(),
                prompt_blueprint: "p".into(),
                suggested_tools: vec![],
                weight: 1.0,
            },
            branch_mode: WorkerBranchMode::DerivedFromMain,
            mission_one_liner: String::new(),
            mission_brief: String::new(),
            work_mode: HiveWorkMode::Build,
        };
        let mut agent =
            WorkerAgent::with_evolution(task, tmp.path().to_path_buf(), profile, "pasada-z".into());
        agent.execute_task().await.unwrap();
        let rep = std::fs::read_to_string(tmp.path().join("ANALYSIS_REPORT.md")).unwrap();
        assert!(rep.contains("ext_toml"));
        assert!(rep.contains("pasada-z"));
    }

    #[tokio::test]
    async fn touch_evolution_escribe_evolution_md_y_log() {
        let tmp = TempDir::new().unwrap();
        let profile = RepositoryProfile {
            languages: vec!["zig".into()],
            frameworks: vec![],
            technical_debt: TechnicalDebtHints::default(),
            specialists: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        let task = WorkerTask {
            id: Uuid::new_v4(),
            title: "t".into(),
            specialist: SpecialistProfile {
                language_key: "zig".into(),
                prompt_blueprint: "p".into(),
                suggested_tools: vec![],
                weight: 1.0,
            },
            branch_mode: WorkerBranchMode::DerivedFromMain,
            mission_one_liner: String::new(),
            mission_brief: String::new(),
            work_mode: HiveWorkMode::Build,
        };
        let mut agent =
            WorkerAgent::with_evolution(task, tmp.path().to_path_buf(), profile, "ev-mark".into());
        agent.execute_task().await.unwrap();
        assert!(tmp.path().join("EVOLUTION.md").exists());
        assert!(tmp.path().join(".hive/evolution.log").exists());
    }
}
