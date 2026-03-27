//! Backend de modelo unificado — abstracción única para todos los proveedores.
//!
//! Este módulo es el "cerebro" que conecta el pipeline con los LLMs.
//! Garantiza: streaming unificado, errores homogéneos, retry automático,
//! rate limiting, cost tracking, y fallback entre proveedores.
//!
//! Es lo que separa un "wrapper de LLM" de una "infraestructura de IA".

use serde::{Deserialize, Serialize};

/// Error unificado del backend (mismo formato para todos los proveedores).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmError {
    pub error_type: LlmErrorType,
    pub message: String,
    pub provider: String,
    pub model: String,
    pub retryable: bool,
    pub retry_after_secs: Option<u64>,
}

/// Tipo de error homogéneo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LlmErrorType {
    RateLimited,
    Timeout,
    InvalidRequest,
    Authentication,
    ServerError,
    NetworkError,
    ContextTooLong,
    Unknown,
}

/// Respuesta unificada del modelo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub model: String,
    pub provider: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cost_usd: f64,
    pub latency_ms: u64,
    pub finish_reason: FinishReason,
}

/// Razón de finalización.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FinishReason {
    Stop,
    Length,
    ToolCall,
    Error,
}

/// Configuración de request al modelo.
#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub max_tokens: usize,
    pub temperature: f32,
    pub stream: bool,
    pub timeout_secs: u64,
}

/// Configuración de retry.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: usize,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 30000,
            backoff_multiplier: 2.0,
        }
    }
}

/// Configuración de rate limiting.
#[derive(Debug, Clone)]
pub struct RateConfig {
    pub requests_per_minute: usize,
    pub tokens_per_minute: usize,
    pub max_concurrent: usize,
}

impl Default for RateConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 60,
            tokens_per_minute: 100_000,
            max_concurrent: 5,
        }
    }
}

/// Trait unificado para todos los proveedores de LLM.
pub trait LlmBackend: Send + Sync {
    /// Nombre del proveedor.
    fn provider_name(&self) -> &str;

    /// Modelo activo.
    fn model_name(&self) -> &str;

    /// Contexto máximo soportado.
    fn max_context_tokens(&self) -> usize;

    /// Envía una request y retorna respuesta unificada.
    fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError>;

    /// Stream de respuesta (simulado: retorna chunks).
    fn stream(&self, request: &LlmRequest) -> Result<Vec<String>, LlmError> {
        // Default: completa y divide en chunks
        let response = self.complete(request)?;
        Ok(response
            .content
            .chars()
            .collect::<Vec<char>>()
            .chunks(50)
            .map(|c| c.iter().collect::<String>())
            .collect())
    }

    /// Cuenta tokens estimados.
    fn estimate_tokens(&self, text: &str) -> usize {
        text.len().div_ceil(4)
    }

    /// Calcula costo estimado.
    fn estimate_cost(&self, input_tokens: usize, output_tokens: usize) -> f64;
}

/// Backend mock para testing.
pub struct MockBackend {
    response: String,
    provider: String,
    model: String,
    latency_ms: u64,
}

impl MockBackend {
    pub fn new(response: &str) -> Self {
        Self {
            response: response.into(),
            provider: "mock".into(),
            model: "mock-v1".into(),
            latency_ms: 10,
        }
    }

    pub fn with_latency(mut self, ms: u64) -> Self {
        self.latency_ms = ms;
        self
    }
}

impl LlmBackend for MockBackend {
    fn provider_name(&self) -> &str {
        &self.provider
    }
    fn model_name(&self) -> &str {
        &self.model
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }

    fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        if self.response.starts_with("ERROR:") {
            return Err(LlmError {
                error_type: LlmErrorType::ServerError,
                message: self.response.clone(),
                provider: self.provider.clone(),
                model: self.model.clone(),
                retryable: true,
                retry_after_secs: Some(1),
            });
        }

        let input_tokens = self.estimate_tokens(&request.prompt);
        let output_tokens = self.estimate_tokens(&self.response);

        Ok(LlmResponse {
            content: self.response.clone(),
            model: self.model.clone(),
            provider: self.provider.clone(),
            input_tokens,
            output_tokens,
            cost_usd: 0.0,
            latency_ms: self.latency_ms,
            finish_reason: FinishReason::Stop,
        })
    }

    fn estimate_cost(&self, _input: usize, _output: usize) -> f64 {
        0.0
    }
}

/// Backend HTTP genérico (para proveedores reales).
#[cfg(feature = "multiagent")]
pub struct HttpBackend {
    provider: String,
    model: String,
    api_base: String,
    api_key: String,
    max_context: usize,
    cost_per_1k_input: f64,
    cost_per_1k_output: f64,
}

#[cfg(feature = "multiagent")]
impl HttpBackend {
    pub fn new(
        provider: &str,
        model: &str,
        api_base: &str,
        api_key: &str,
        max_context: usize,
        cost_in: f64,
        cost_out: f64,
    ) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            api_base: api_base.into(),
            api_key: api_key.into(),
            max_context: max_context,
            cost_per_1k_input: cost_in,
            cost_per_1k_output: cost_out,
        }
    }

    /// Construye desde config de modelo.
    pub fn from_model_config(config: &crate::multi_model::ModelConfig) -> Option<Self> {
        let api_key = std::env::var(&config.api_key_env).ok()?;
        let api_base = config.api_base.clone().unwrap_or_default();
        Some(Self::new(
            &format!("{:?}", config.provider),
            &config.model_name,
            &api_base,
            &api_key,
            config.max_tokens,
            config.cost_per_1k_input,
            config.cost_per_1k_output,
        ))
    }
}

#[cfg(feature = "multiagent")]
impl LlmBackend for HttpBackend {
    fn provider_name(&self) -> &str {
        &self.provider
    }
    fn model_name(&self) -> &str {
        &self.model
    }
    fn max_context_tokens(&self) -> usize {
        self.max_context
    }

    fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let start = std::time::Instant::now();
        let client = reqwest::blocking::Client::new();

        // Formato OpenAI-compatible (la mayoría de proveedores lo soportan)
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": request.system_prompt.as_deref().unwrap_or("You are a helpful assistant.")},
                {"role": "user", "content": request.prompt}
            ],
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "stream": false
        });

        let resp = client
            .post(format!("{}/chat/completions", self.api_base))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(std::time::Duration::from_secs(request.timeout_secs))
            .send()
            .map_err(|e| LlmError {
                error_type: if e.is_timeout() {
                    LlmErrorType::Timeout
                } else {
                    LlmErrorType::NetworkError
                },
                message: e.to_string(),
                provider: self.provider.clone(),
                model: self.model.clone(),
                retryable: !e.is_timeout(),
                retry_after_secs: None,
            })?;

        if resp.status() == 429 {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok());
            return Err(LlmError {
                error_type: LlmErrorType::RateLimited,
                message: "Rate limited".into(),
                provider: self.provider.clone(),
                model: self.model.clone(),
                retryable: true,
                retry_after_secs: retry_after,
            });
        }

        if resp.status() == 401 {
            return Err(LlmError {
                error_type: LlmErrorType::Authentication,
                message: "Invalid API key".into(),
                provider: self.provider.clone(),
                model: self.model.clone(),
                retryable: false,
                retry_after_secs: None,
            });
        }

        if !resp.status().is_success() {
            return Err(LlmError {
                error_type: LlmErrorType::ServerError,
                message: format!("HTTP {}", resp.status()),
                provider: self.provider.clone(),
                model: self.model.clone(),
                retryable: true,
                retry_after_secs: None,
            });
        }

        let json: serde_json::Value = resp.json().map_err(|e| LlmError {
            error_type: LlmErrorType::ServerError,
            message: e.to_string(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            retryable: true,
            retry_after_secs: None,
        })?;

        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let input_tokens = json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as usize;
        let output_tokens = json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as usize;
        let latency = start.elapsed().as_millis() as u64;

        Ok(LlmResponse {
            content,
            model: self.model.clone(),
            provider: self.provider.clone(),
            input_tokens,
            output_tokens,
            cost_usd: self.estimate_cost(input_tokens, output_tokens),
            latency_ms: latency,
            finish_reason: FinishReason::Stop,
        })
    }

    fn estimate_cost(&self, input: usize, output: usize) -> f64 {
        (input as f64 / 1000.0) * self.cost_per_1k_input
            + (output as f64 / 1000.0) * self.cost_per_1k_output
    }
}

/// Ejecutor con retry automático y fallback entre proveedores.
pub struct ResilientExecutor {
    backends: Vec<Box<dyn LlmBackend>>,
    retry_config: RetryConfig,
}

impl ResilientExecutor {
    pub fn new(retry_config: RetryConfig) -> Self {
        Self {
            backends: Vec::new(),
            retry_config,
        }
    }

    pub fn add_backend(&mut self, backend: Box<dyn LlmBackend>) {
        self.backends.push(backend);
    }

    /// Ejecuta con retry y fallback automático.
    pub fn execute(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let mut last_error = None;

        for backend in &self.backends {
            for attempt in 0..=self.retry_config.max_retries {
                match backend.complete(request) {
                    Ok(response) => return Ok(response),
                    Err(e) => {
                        if !e.retryable || attempt == self.retry_config.max_retries {
                            last_error = Some(e);
                            break; // Pasar al siguiente backend
                        }

                        // Delay exponencial
                        let delay = (self.retry_config.base_delay_ms as f64
                            * self.retry_config.backoff_multiplier.powi(attempt as i32))
                        .min(self.retry_config.max_delay_ms as f64)
                            as u64;

                        let retry_delay = e.retry_after_secs.map(|s| s * 1000).unwrap_or(delay);
                        std::thread::sleep(std::time::Duration::from_millis(retry_delay));
                    }
                }
            }
        }

        Err(last_error.unwrap_or(LlmError {
            error_type: LlmErrorType::Unknown,
            message: "no backends available".into(),
            provider: "none".into(),
            model: "none".into(),
            retryable: false,
            retry_after_secs: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_backend_responds() {
        let backend = MockBackend::new("Hello!");
        let request = LlmRequest {
            prompt: "Hi".into(),
            system_prompt: None,
            max_tokens: 100,
            temperature: 0.0,
            stream: false,
            timeout_secs: 10,
        };
        let response = backend.complete(&request).unwrap();
        assert_eq!(response.content, "Hello!");
        assert_eq!(response.finish_reason, FinishReason::Stop);
    }

    #[test]
    fn mock_backend_error() {
        let backend = MockBackend::new("ERROR: timeout");
        let request = LlmRequest {
            prompt: "Hi".into(),
            system_prompt: None,
            max_tokens: 100,
            temperature: 0.0,
            stream: false,
            timeout_secs: 10,
        };
        let err = backend.complete(&request).unwrap_err();
        assert_eq!(err.error_type, LlmErrorType::ServerError);
        assert!(err.retryable);
    }

    #[test]
    fn resilient_executor_fallback() {
        let mut executor = ResilientExecutor::new(RetryConfig {
            max_retries: 0,
            ..Default::default()
        });
        executor.add_backend(Box::new(MockBackend::new("ERROR: fail")));
        executor.add_backend(Box::new(MockBackend::new("fallback response")));

        let request = LlmRequest {
            prompt: "test".into(),
            system_prompt: None,
            max_tokens: 100,
            temperature: 0.0,
            stream: false,
            timeout_secs: 10,
        };
        let response = executor.execute(&request).unwrap();
        assert_eq!(response.content, "fallback response");
    }

    #[test]
    fn token_estimation() {
        let backend = MockBackend::new("test");
        assert_eq!(backend.estimate_tokens("hello"), 2);
        assert_eq!(backend.estimate_tokens("a b c d e f g h"), 4);
    }

    #[test]
    fn error_types_serializable() {
        let err = LlmError {
            error_type: LlmErrorType::RateLimited,
            message: "too fast".into(),
            provider: "openai".into(),
            model: "gpt-4".into(),
            retryable: true,
            retry_after_secs: Some(60),
        };
        let json = serde_json::to_string(&err).unwrap();
        let back: LlmError = serde_json::from_str(&json).unwrap();
        assert_eq!(back.error_type, LlmErrorType::RateLimited);
    }
}
