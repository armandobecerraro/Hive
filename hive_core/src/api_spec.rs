//! REST API para integraciones CI/CD, webhooks.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEndpoint {
    pub method: String,
    pub path: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    pub status: u16,
    pub body: String,
}

pub struct ApiSpec;

impl ApiSpec {
    pub fn endpoints() -> Vec<ApiEndpoint> {
        vec![
            ApiEndpoint {
                method: "GET".into(),
                path: "/health".into(),
                description: "Health check".into(),
            },
            ApiEndpoint {
                method: "GET".into(),
                path: "/status".into(),
                description: "Current status".into(),
            },
            ApiEndpoint {
                method: "POST".into(),
                path: "/cycle".into(),
                description: "Trigger a cycle".into(),
            },
            ApiEndpoint {
                method: "GET".into(),
                path: "/metrics".into(),
                description: "Prometheus metrics".into(),
            },
            ApiEndpoint {
                method: "GET".into(),
                path: "/workers".into(),
                description: "List active workers".into(),
            },
            ApiEndpoint {
                method: "POST".into(),
                path: "/webhook/github".into(),
                description: "GitHub webhook".into(),
            },
            ApiEndpoint {
                method: "GET".into(),
                path: "/history".into(),
                description: "Decision history".into(),
            },
            ApiEndpoint {
                method: "GET".into(),
                path: "/version".into(),
                description: "Current version".into(),
            },
        ]
    }

    pub fn openapi_spec() -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "openapi": "3.1.0",
            "info": { "title": "Hive API", "version": "0.1.0" },
            "paths": {
                "/health": { "get": { "summary": "Health check", "responses": { "200": { "description": "OK" } } } },
                "/status": { "get": { "summary": "Current status", "responses": { "200": { "description": "Status" } } } },
                "/cycle": { "post": { "summary": "Trigger cycle", "responses": { "200": { "description": "Started" } } } },
                "/metrics": { "get": { "summary": "Prometheus metrics", "responses": { "200": { "description": "Metrics" } } } }
            }
        })).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_exist() {
        assert!(ApiSpec::endpoints().len() >= 8);
    }

    #[test]
    fn openapi_spec_valid_json() {
        let spec = ApiSpec::openapi_spec();
        assert!(serde_json::from_str::<serde_json::Value>(&spec).is_ok());
    }
}
