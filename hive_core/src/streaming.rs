//! Real-time streaming de respuestas LLM.
//!
//! Permite recibir tokens del LLM conforme se generan, habilitando UX interactiva.
//! Soporta SSE (Server-Sent Events) y streaming por chunks.

use serde::{Deserialize, Serialize};

/// Chunk de streaming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub content: String,
    pub is_final: bool,
    pub tokens_so_far: usize,
    pub elapsed_ms: u64,
}

/// Estado del streaming.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StreamState {
    Idle,
    Streaming,
    Complete,
    Error,
}

/// Métricas de streaming.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StreamMetrics {
    pub total_tokens: usize,
    pub total_chars: usize,
    pub duration_ms: u64,
    pub tokens_per_second: f64,
    pub first_token_ms: u64,
}

/// Colector de chunks de streaming.
pub struct StreamCollector {
    state: StreamState,
    buffer: String,
    chunks: Vec<StreamChunk>,
    start_time: Option<std::time::Instant>,
    first_token_time: Option<std::time::Instant>,
    total_tokens: usize,
}

impl Default for StreamCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamCollector {
    pub fn new() -> Self {
        Self {
            state: StreamState::Idle,
            buffer: String::new(),
            chunks: Vec::new(),
            start_time: None,
            first_token_time: None,
            total_tokens: 0,
        }
    }

    /// Inicia el streaming.
    pub fn start(&mut self) {
        self.state = StreamState::Streaming;
        self.buffer.clear();
        self.chunks.clear();
        self.start_time = Some(std::time::Instant::now());
        self.first_token_time = None;
        self.total_tokens = 0;
    }

    /// Procesa un chunk entrante.
    pub fn push(&mut self, content: &str) -> StreamChunk {
        if self.first_token_time.is_none() && !content.is_empty() {
            self.first_token_time = Some(std::time::Instant::now());
        }

        self.buffer.push_str(content);
        self.total_tokens += 1;

        let elapsed = self
            .start_time
            .map(|s| s.elapsed().as_millis() as u64)
            .unwrap_or(0);

        let chunk = StreamChunk {
            content: content.to_string(),
            is_final: false,
            tokens_so_far: self.total_tokens,
            elapsed_ms: elapsed,
        };
        self.chunks.push(chunk.clone());
        chunk
    }

    /// Finaliza el streaming.
    pub fn finish(&mut self) -> StreamChunk {
        self.state = StreamState::Complete;
        let elapsed = self
            .start_time
            .map(|s| s.elapsed().as_millis() as u64)
            .unwrap_or(0);

        let chunk = StreamChunk {
            content: String::new(),
            is_final: true,
            tokens_so_far: self.total_tokens,
            elapsed_ms: elapsed,
        };
        self.chunks.push(chunk.clone());
        chunk
    }

    /// Marca como error.
    pub fn error(&mut self, msg: &str) {
        self.state = StreamState::Error;
        self.buffer.push_str(&format!("\n[ERROR: {msg}]"));
    }

    /// Obtiene el contenido completo acumulado.
    pub fn content(&self) -> &str {
        &self.buffer
    }

    /// Obtiene el estado actual.
    pub fn state(&self) -> &StreamState {
        &self.state
    }

    /// Calcula métricas finales.
    pub fn metrics(&self) -> StreamMetrics {
        let duration = self
            .start_time
            .map(|s| s.elapsed().as_millis() as u64)
            .unwrap_or(0);
        let first_token = self
            .first_token_time
            .and_then(|f| {
                self.start_time
                    .map(|s| f.duration_since(s).as_millis() as u64)
            })
            .unwrap_or(0);
        let tps = if duration > 0 {
            self.total_tokens as f64 * 1000.0 / duration as f64
        } else {
            0.0
        };

        StreamMetrics {
            total_tokens: self.total_tokens,
            total_chars: self.buffer.len(),
            duration_ms: duration,
            tokens_per_second: tps,
            first_token_ms: first_token,
        }
    }
}

/// Simula streaming desde un texto completo (para testing/mock).
pub fn simulate_stream(text: &str, chunk_size: usize) -> Vec<String> {
    text.chars()
        .collect::<Vec<char>>()
        .chunks(chunk_size)
        .map(|c| c.iter().collect::<String>())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_tracks_content() {
        let mut c = StreamCollector::new();
        c.start();
        c.push("hello ");
        c.push("world");
        c.finish();
        assert_eq!(c.content(), "hello world");
        assert_eq!(*c.state(), StreamState::Complete);
    }

    #[test]
    fn collector_metrics() {
        let mut c = StreamCollector::new();
        c.start();
        c.push("a");
        c.push("b");
        c.push("c");
        c.finish();
        let m = c.metrics();
        assert_eq!(m.total_tokens, 3);
        assert_eq!(m.total_chars, 3);
    }

    #[test]
    fn simulate_stream_splits_text() {
        let chunks = simulate_stream("hello world", 5);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks.concat(), "hello world");
    }
}
