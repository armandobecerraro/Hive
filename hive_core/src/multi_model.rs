//! Soporte multi-modelo para LLMs.
//!
//! Permite usar OpenAI, Anthropic, Google, modelos locales (Ollama/llama.cpp),
//! y selección dinámica por tarea. Inspirado en LiteLLM y Aider que abstraen múltiples proveedores.

use serde::{Deserialize, Serialize};

/// Proveedor de LLM.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LLMProvider {
    OpenAI,
    Anthropic,
    Google,
    Ollama,
    Local,
    Azure,
    Groq,
    Together,
}

/// Configuración de un modelo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: LLMProvider,
    pub model_name: String,
    pub api_base: Option<String>,
    pub api_key_env: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub cost_per_1k_input: f64,
    pub cost_per_1k_output: f64,
}

/// Registro de modelos disponibles.
pub struct ModelRegistry {
    models: Vec<ModelConfig>,
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            models: Self::default_models(),
        }
    }

    fn default_models() -> Vec<ModelConfig> {
        vec![
            ModelConfig {
                provider: LLMProvider::OpenAI,
                model_name: "gpt-4o".into(),
                api_base: Some("https://api.openai.com/v1".into()),
                api_key_env: "OPENAI_API_KEY".into(),
                max_tokens: 128_000,
                temperature: 0.0,
                cost_per_1k_input: 0.005,
                cost_per_1k_output: 0.015,
            },
            ModelConfig {
                provider: LLMProvider::Anthropic,
                model_name: "claude-sonnet-4-20250514".into(),
                api_base: Some("https://api.anthropic.com".into()),
                api_key_env: "ANTHROPIC_API_KEY".into(),
                max_tokens: 200_000,
                temperature: 0.0,
                cost_per_1k_input: 0.003,
                cost_per_1k_output: 0.015,
            },
            ModelConfig {
                provider: LLMProvider::Google,
                model_name: "gemini-2.5-pro".into(),
                api_base: Some("https://generativelanguage.googleapis.com".into()),
                api_key_env: "GOOGLE_API_KEY".into(),
                max_tokens: 1_000_000,
                temperature: 0.0,
                cost_per_1k_input: 0.00125,
                cost_per_1k_output: 0.005,
            },
            ModelConfig {
                provider: LLMProvider::Ollama,
                model_name: "codellama".into(),
                api_base: Some("http://localhost:11434".into()),
                api_key_env: String::new(),
                max_tokens: 16_000,
                temperature: 0.0,
                cost_per_1k_input: 0.0,
                cost_per_1k_output: 0.0,
            },
            ModelConfig {
                provider: LLMProvider::Groq,
                model_name: "llama-3.1-70b-versatile".into(),
                api_base: Some("https://api.groq.com/openai/v1".into()),
                api_key_env: "GROQ_API_KEY".into(),
                max_tokens: 131_072,
                temperature: 0.0,
                cost_per_1k_input: 0.00059,
                cost_per_1k_output: 0.00079,
            },
            ModelConfig {
                provider: LLMProvider::Together,
                model_name: "meta-llama/Llama-3-70b-chat-hf".into(),
                api_base: Some("https://api.together.xyz".into()),
                api_key_env: "TOGETHER_API_KEY".into(),
                max_tokens: 8_192,
                temperature: 0.0,
                cost_per_1k_input: 0.0009,
                cost_per_1k_output: 0.0009,
            },
        ]
    }

    /// Registra un modelo personalizado.
    pub fn register(&mut self, config: ModelConfig) {
        self.models.push(config);
    }

    /// Selecciona el mejor modelo según criterios.
    pub fn select_best(&self, criteria: &SelectionCriteria) -> Option<&ModelConfig> {
        let mut candidates: Vec<&ModelConfig> = self
            .models
            .iter()
            .filter(|m| {
                if let Some(ref provider) = criteria.provider {
                    if &m.provider != provider {
                        return false;
                    }
                }
                if let Some(max_cost) = criteria.max_cost_per_1k {
                    if m.cost_per_1k_input > max_cost {
                        return false;
                    }
                }
                if let Some(min_context) = criteria.min_context_window {
                    if m.max_tokens < min_context {
                        return false;
                    }
                }
                // Verificar que la API key existe si es necesario
                if m.provider != LLMProvider::Ollama
                    && m.provider != LLMProvider::Local
                    && !m.api_key_env.is_empty()
                    && std::env::var(&m.api_key_env).is_err()
                {
                    return false;
                }
                true
            })
            .collect();

        // Priorizar por contexto más grande y menor costo
        candidates.sort_by(|a, b| {
            let score_a = a.max_tokens as f64 / (a.cost_per_1k_input + 0.001);
            let score_b = b.max_tokens as f64 / (b.cost_per_1k_input + 0.001);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        candidates.first().copied()
    }

    /// Lista todos los modelos disponibles.
    pub fn list_available(&self) -> Vec<&ModelConfig> {
        self.models
            .iter()
            .filter(|m| {
                if m.provider == LLMProvider::Ollama || m.provider == LLMProvider::Local {
                    return true;
                }
                !m.api_key_env.is_empty() && std::env::var(&m.api_key_env).is_ok()
            })
            .collect()
    }

    /// Obtiene un modelo por nombre.
    pub fn get_by_name(&self, name: &str) -> Option<&ModelConfig> {
        self.models.iter().find(|m| m.model_name == name)
    }
}

/// Criterios de selección de modelo.
#[derive(Debug, Clone, Default)]
pub struct SelectionCriteria {
    pub provider: Option<LLMProvider>,
    pub max_cost_per_1k: Option<f64>,
    pub min_context_window: Option<usize>,
    pub prefer_local: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_defaults() {
        let reg = ModelRegistry::new();
        assert!(reg.models.len() >= 6);
    }

    #[test]
    fn get_by_name_works() {
        let reg = ModelRegistry::new();
        assert!(reg.get_by_name("gpt-4o").is_some());
        assert!(reg.get_by_name("nonexistent").is_none());
    }

    #[test]
    fn select_best_ollama_always_available() {
        let reg = ModelRegistry::new();
        let criteria = SelectionCriteria {
            provider: Some(LLMProvider::Ollama),
            ..Default::default()
        };
        let selected = reg.select_best(&criteria);
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().provider, LLMProvider::Ollama);
    }
}
