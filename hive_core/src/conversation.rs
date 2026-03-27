//! Historial de conversación entre humano y agente.
//!
//! Mantiene un registro de turnos (humano/assistente) con contexto persistente
//! entre ciclos. Inspirado en Cursor y Copilot Chat que mantienen contexto conversacional.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Rol en la conversación.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Un mensaje individual.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: i64,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Historial de conversación.
pub struct ConversationHistory {
    messages: Vec<ConversationMessage>,
    path: PathBuf,
    max_messages: usize,
}

impl ConversationHistory {
    pub fn new(repo_root: &Path, max_messages: usize) -> Self {
        let path = repo_root.join(".hive").join("conversation.json");
        let messages = Self::load_from_disk(&path).unwrap_or_default();
        Self {
            messages,
            path,
            max_messages,
        }
    }

    fn load_from_disk(path: &Path) -> anyhow::Result<Vec<ConversationMessage>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let raw = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.messages)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    /// Añade un mensaje del usuario.
    pub fn add_user_message(&mut self, content: &str) {
        self.messages.push(ConversationMessage {
            role: MessageRole::User,
            content: content.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            metadata: std::collections::HashMap::new(),
        });
        self.trim();
        let _ = self.save();
    }

    /// Añade un mensaje del asistente.
    pub fn add_assistant_message(&mut self, content: &str) {
        self.messages.push(ConversationMessage {
            role: MessageRole::Assistant,
            content: content.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            metadata: std::collections::HashMap::new(),
        });
        self.trim();
        let _ = self.save();
    }

    /// Añade un mensaje del sistema.
    pub fn add_system_message(&mut self, content: &str) {
        self.messages.push(ConversationMessage {
            role: MessageRole::System,
            content: content.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            metadata: std::collections::HashMap::new(),
        });
    }

    /// Obtiene los últimos N mensajes.
    pub fn recent(&self, n: usize) -> &[ConversationMessage] {
        let start = if self.messages.len() > n {
            self.messages.len() - n
        } else {
            0
        };
        &self.messages[start..]
    }

    /// Obtiene todos los mensajes.
    pub fn all(&self) -> &[ConversationMessage] {
        &self.messages
    }

    /// Formatea el historial para el prompt del LLM.
    pub fn format_for_prompt(&self, max_turns: usize) -> String {
        let recent = self.recent(max_turns);
        let mut result = String::new();
        for msg in recent {
            let role_str = match msg.role {
                MessageRole::System => "System",
                MessageRole::User => "Human",
                MessageRole::Assistant => "Assistant",
                MessageRole::Tool => "Tool",
            };
            result.push_str(&format!("[{role_str}]: {}\n\n", msg.content));
        }
        result
    }

    /// Limpia el historial.
    pub fn clear(&mut self) {
        self.messages.clear();
        let _ = self.save();
    }

    fn trim(&mut self) {
        if self.messages.len() > self.max_messages {
            let excess = self.messages.len() - self.max_messages;
            self.messages.drain(..excess);
        }
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_adds_messages() {
        let tmp = tempfile::tempdir().unwrap();
        let mut conv = ConversationHistory::new(tmp.path(), 100);
        conv.add_user_message("Hola");
        conv.add_assistant_message("Hola! ¿En qué puedo ayudarte?");
        assert_eq!(conv.len(), 2);
    }

    #[test]
    fn conversation_limits_messages() {
        let tmp = tempfile::tempdir().unwrap();
        let mut conv = ConversationHistory::new(tmp.path(), 3);
        for i in 0..5 {
            conv.add_user_message(&format!("msg {i}"));
        }
        assert_eq!(conv.len(), 3);
    }

    #[test]
    fn format_for_prompt_works() {
        let tmp = tempfile::tempdir().unwrap();
        let mut conv = ConversationHistory::new(tmp.path(), 100);
        conv.add_user_message("test");
        let formatted = conv.format_for_prompt(10);
        assert!(formatted.contains("[Human]: test"));
    }

    #[test]
    fn conversation_persists() {
        let tmp = tempfile::tempdir().unwrap();
        {
            let mut conv = ConversationHistory::new(tmp.path(), 100);
            conv.add_user_message("persist test");
        }
        let conv = ConversationHistory::new(tmp.path(), 100);
        assert_eq!(conv.len(), 1);
    }
}
