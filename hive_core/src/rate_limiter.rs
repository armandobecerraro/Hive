//! Rate limiting para llamadas LLM.
//!
//! Controla la tasa de requests para no agotar cuota API ni sobrecargar modelos locales.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuración de rate limit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub max_requests_per_minute: usize,
    pub max_tokens_per_minute: usize,
    pub max_concurrent: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests_per_minute: 60,
            max_tokens_per_minute: 100_000,
            max_concurrent: 5,
        }
    }
}

/// Estado de rate limiting por proveedor.
pub struct RateLimiter {
    configs: HashMap<String, RateLimitConfig>,
    request_counts: HashMap<String, Vec<i64>>,
    token_counts: HashMap<String, Vec<(i64, usize)>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
            request_counts: HashMap::new(),
            token_counts: HashMap::new(),
        }
    }

    /// Configura rate limit para un proveedor.
    pub fn configure(&mut self, provider: &str, config: RateLimitConfig) {
        self.configs.insert(provider.into(), config);
    }

    /// Verifica si se puede hacer una request.
    pub fn can_request(&self, provider: &str) -> bool {
        let config = self.configs.get(provider);
        if config.is_none() {
            return true;
        }

        let now = chrono::Utc::now().timestamp();
        let window_start = now - 60;

        let request_count = self
            .request_counts
            .get(provider)
            .map(|v| v.iter().filter(|&&t| t > window_start).count())
            .unwrap_or(0);

        request_count < config.unwrap().max_requests_per_minute
    }

    /// Registra una request.
    pub fn record_request(&mut self, provider: &str, tokens: usize) {
        let now = chrono::Utc::now().timestamp();
        self.request_counts
            .entry(provider.into())
            .or_default()
            .push(now);
        self.token_counts
            .entry(provider.into())
            .or_default()
            .push((now, tokens));
    }

    /// Espera si es necesario (async).
    pub async fn wait_if_needed(&self, provider: &str) {
        while !self.can_request(provider) {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_limit_without_config() {
        let limiter = RateLimiter::new();
        assert!(limiter.can_request("openai"));
    }

    #[test]
    fn rate_limit_works() {
        let mut limiter = RateLimiter::new();
        limiter.configure(
            "test",
            RateLimitConfig {
                max_requests_per_minute: 2,
                ..Default::default()
            },
        );
        limiter.record_request("test", 100);
        limiter.record_request("test", 100);
        assert!(!limiter.can_request("test"));
    }
}
