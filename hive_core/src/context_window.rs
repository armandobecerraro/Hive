//! Gestión de ventana de contexto para LLMs.
//!
//! Implementa chunking inteligente, summarización y selección relevante de archivos
//! para que las obreras puedan trabajar con repos grandes sin exceder el límite de tokens.
//! Inspirado en Cursor y Aider que seleccionan archivos relevantes al contexto.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuración de ventana de contexto.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindowConfig {
    pub max_tokens: usize,
    pub max_files: usize,
    pub chunk_size: usize,
    pub overlap: usize,
    pub summarize_threshold: f32,
}

impl Default for ContextWindowConfig {
    fn default() -> Self {
        Self {
            max_tokens: 128_000,
            max_files: 50,
            chunk_size: 2000,
            overlap: 200,
            summarize_threshold: 0.8,
        }
    }
}

/// Chunk de código con metadatos.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunk {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub token_estimate: usize,
    pub relevance_score: f32,
    pub chunk_type: ChunkType,
}

/// Tipo de chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChunkType {
    Function,
    Class,
    Import,
    Module,
    Comment,
    Config,
}

/// Contexto compilado para enviar al LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledContext {
    pub chunks: Vec<CodeChunk>,
    pub total_tokens: usize,
    pub files_included: usize,
    pub files_truncated: usize,
    pub summary: Option<String>,
}

/// Motor de gestión de contexto.
pub struct ContextWindow {
    config: ContextWindowConfig,
    file_contents: HashMap<String, String>,
}

impl ContextWindow {
    pub fn new(config: ContextWindowConfig) -> Self {
        Self {
            config,
            file_contents: HashMap::new(),
        }
    }

    /// Añade un archivo al pool de contexto.
    pub fn add_file(&mut self, path: &str, content: &str) {
        self.file_contents
            .insert(path.to_string(), content.to_string());
    }

    /// Estima tokens (aprox 4 chars por token).
    pub fn estimate_tokens(text: &str) -> usize {
        text.len().div_ceil(4)
    }

    /// Chunking de un archivo en bloques inteligentes.
    pub fn chunk_file(&self, path: &str, content: &str) -> Vec<CodeChunk> {
        let lines: Vec<&str> = content.lines().collect();
        let mut chunks = Vec::new();
        let mut current_start = 0;
        let mut current_lines: Vec<&str> = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            current_lines.push(line);

            // Detectar límites naturales: funciones, clases, imports
            let is_boundary = line.starts_with("fn ")
                || line.starts_with("pub fn ")
                || line.starts_with("class ")
                || line.starts_with("def ")
                || line.starts_with("function ")
                || line.starts_with("import ")
                || line.starts_with("use ")
                || line.starts_with("from ");

            let chunk_text = current_lines.join("\n");
            let tokens = Self::estimate_tokens(&chunk_text);

            if ((is_boundary && tokens > self.config.chunk_size / 2)
                || tokens >= self.config.chunk_size
                || i == lines.len() - 1)
                && !current_lines.is_empty()
            {
                let chunk_type = detect_chunk_type(&chunk_text);
                chunks.push(CodeChunk {
                    file_path: path.to_string(),
                    start_line: current_start + 1,
                    end_line: i + 1,
                    content: chunk_text.clone(),
                    token_estimate: Self::estimate_tokens(&chunk_text),
                    relevance_score: 0.0,
                    chunk_type,
                });

                // Overlap para continuidad
                let overlap_lines = self.config.overlap / 80; // ~80 chars per line
                if overlap_lines > 0 && i + 1 < lines.len() {
                    let overlap_start = i.saturating_sub(overlap_lines);
                    current_start = overlap_start;
                    current_lines = lines[overlap_start..=i].to_vec();
                } else {
                    current_start = i + 1;
                    current_lines.clear();
                }
            }
        }

        chunks
    }

    /// Compila contexto relevante para una tarea dada.
    pub fn compile_context(
        &self,
        task_description: &str,
        max_tokens: Option<usize>,
    ) -> CompiledContext {
        let limit = max_tokens.unwrap_or(self.config.max_tokens);
        let mut all_chunks = Vec::new();

        // Chunk todos los archivos
        for (path, content) in &self.file_contents {
            let chunks = self.chunk_file(path, content);
            all_chunks.extend(chunks);
        }

        // Calcular relevancia por similitud de términos
        let task_words: std::collections::HashSet<&str> =
            task_description.split_whitespace().collect();
        for chunk in &mut all_chunks {
            let chunk_words: std::collections::HashSet<&str> =
                chunk.content.split_whitespace().collect();
            let intersection = task_words.intersection(&chunk_words).count();
            let union = task_words.union(&chunk_words).count();
            chunk.relevance_score = if union > 0 {
                intersection as f32 / union as f32
            } else {
                0.0
            };

            // Boost por filename relevance
            if task_description.contains(&chunk.file_path) {
                chunk.relevance_score += 0.5;
            }
        }

        // Ordenar por relevancia
        all_chunks.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Seleccionar chunks que caben en la ventana
        let mut selected = Vec::new();
        let mut total_tokens = 0usize;
        let mut included_files: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for chunk in all_chunks {
            if total_tokens + chunk.token_estimate > limit {
                break;
            }
            if selected.len() >= self.config.max_files * 3 {
                break;
            }
            total_tokens += chunk.token_estimate;
            included_files.insert(chunk.file_path.clone());
            selected.push(chunk);
        }

        let files_truncated = self
            .file_contents
            .len()
            .saturating_sub(included_files.len());

        CompiledContext {
            chunks: selected,
            total_tokens,
            files_included: included_files.len(),
            files_truncated,
            summary: None,
        }
    }

    /// Genera un resumen del contexto cuando se excede el umbral.
    pub fn summarize_context(&self, context: &CompiledContext) -> String {
        let mut summary = String::new();
        summary.push_str(&format!(
            "=== Contexto: {} archivos, ~{} tokens ===\n",
            context.files_included, context.total_tokens
        ));

        let mut files_summary: HashMap<String, (usize, usize)> = HashMap::new();
        for chunk in &context.chunks {
            let entry = files_summary
                .entry(chunk.file_path.clone())
                .or_insert((0, 0));
            entry.0 += 1;
            entry.1 += chunk.token_estimate;
        }

        for (file, (chunks, tokens)) in &files_summary {
            summary.push_str(&format!("  {file}: {chunks} chunks, ~{tokens} tokens\n"));
        }

        summary
    }
}

fn detect_chunk_type(text: &str) -> ChunkType {
    let first_line = text.lines().next().unwrap_or("");
    if first_line.contains("fn ") || first_line.contains("def ") || first_line.contains("function ")
    {
        ChunkType::Function
    } else if first_line.contains("class ")
        || first_line.contains("struct ")
        || first_line.contains("enum ")
    {
        ChunkType::Class
    } else if first_line.starts_with("import ")
        || first_line.starts_with("use ")
        || first_line.starts_with("from ")
    {
        ChunkType::Import
    } else if first_line.starts_with("#[")
        || first_line.starts_with("@")
        || first_line.starts_with("[tool")
    {
        ChunkType::Config
    } else if first_line.starts_with("//")
        || first_line.starts_with("#")
        || first_line.starts_with("/*")
    {
        ChunkType::Comment
    } else {
        ChunkType::Module
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_tokens_basic() {
        assert_eq!(ContextWindow::estimate_tokens("hello"), 2);
        assert_eq!(ContextWindow::estimate_tokens("a b c d"), 2);
    }

    #[test]
    fn chunk_file_creates_chunks() {
        let cw = ContextWindow::new(ContextWindowConfig {
            chunk_size: 50,
            ..Default::default()
        });
        let code = "fn main() {\n    println!(\"hello\");\n}\n\nfn other() {\n    println!(\"world\");\n}\n";
        let chunks = cw.chunk_file("main.rs", code);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn compile_context_respects_limit() {
        let mut cw = ContextWindow::new(ContextWindowConfig {
            max_tokens: 100,
            ..Default::default()
        });
        cw.add_file("a.rs", &"x ".repeat(500));
        cw.add_file("b.rs", &"y ".repeat(500));
        let ctx = cw.compile_context("test x", Some(100));
        assert!(ctx.total_tokens <= 100);
    }

    #[test]
    fn relevance_scoring_works() {
        let mut cw = ContextWindow::new(ContextWindowConfig::default());
        cw.add_file(
            "relevant.rs",
            "fn process_data(data: Vec<u8>) { data.len() }",
        );
        cw.add_file("unrelated.rs", "fn greet() { println!(\"hello\") }");
        let ctx = cw.compile_context("process data vector", None);
        assert!(!ctx.chunks.is_empty());
        // Verificar que el chunk relevante tiene score >= al no relevante
        let relevant_score = ctx
            .chunks
            .iter()
            .find(|c| c.file_path == "relevant.rs")
            .map(|c| c.relevance_score);
        let unrelated_score = ctx
            .chunks
            .iter()
            .find(|c| c.file_path == "unrelated.rs")
            .map(|c| c.relevance_score);
        if let (Some(r), Some(u)) = (relevant_score, unrelated_score) {
            assert!(
                r >= u,
                "relevant score ({r}) should be >= unrelated score ({u})"
            );
        }
    }
}
