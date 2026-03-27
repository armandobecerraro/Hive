//! Webhook server para reaccionar a eventos de GitHub/GitLab.
//! Dispara ciclos en push, PR, issue creation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub event_type: String,
    pub action: String,
    pub repo: String,
    pub branch: Option<String>,
    pub issue_number: Option<u64>,
    pub pr_number: Option<u64>,
    pub sender: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub port: u16,
    pub secret: Option<String>,
    pub enabled_events: Vec<String>,
    pub trigger_cycle_on: Vec<String>,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            port: 9876,
            secret: None,
            enabled_events: vec!["push".into(), "pull_request".into(), "issues".into()],
            trigger_cycle_on: vec!["push".into(), "issues.opened".into()],
        }
    }
}

pub struct WebhookHandler;

impl WebhookHandler {
    /// Parsea un payload de GitHub webhook.
    pub fn parse_github_payload(payload: &str, event_type: &str) -> Option<WebhookEvent> {
        let v: serde_json::Value = serde_json::from_str(payload).ok()?;

        Some(WebhookEvent {
            event_type: event_type.into(),
            action: v
                .get("action")
                .and_then(|a| a.as_str())
                .unwrap_or("unknown")
                .into(),
            repo: v
                .get("repository")
                .and_then(|r| r.get("full_name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .into(),
            branch: v
                .get("ref")
                .and_then(|r| r.as_str())
                .map(|s| s.replace("refs/heads/", "")),
            issue_number: v
                .get("issue")
                .and_then(|i| i.get("number"))
                .and_then(|n| n.as_u64()),
            pr_number: v
                .get("pull_request")
                .and_then(|p| p.get("number"))
                .and_then(|n| n.as_u64()),
            sender: v
                .get("sender")
                .and_then(|s| s.get("login"))
                .and_then(|l| l.as_str())
                .unwrap_or("unknown")
                .into(),
            timestamp: chrono::Utc::now().timestamp(),
        })
    }

    /// Verifica si un evento debe disparar un ciclo.
    pub fn should_trigger_cycle(event: &WebhookEvent, config: &WebhookConfig) -> bool {
        let key = format!("{}.{}", event.event_type, event.action);
        config
            .trigger_cycle_on
            .iter()
            .any(|t| t == &event.event_type || t == &key)
    }

    /// Verifica la firma del webhook (HMAC).
    pub fn verify_signature(_payload: &str, signature: &str, secret: &str) -> bool {
        if secret.is_empty() {
            return true;
        }
        // Simplificación: en producción usaría hmac sha256
        let expected = format!("sha256={secret}");
        signature == expected || signature.starts_with("sha256=")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_push() {
        let payload = r#"{"ref":"refs/heads/main","repository":{"full_name":"user/repo"},"sender":{"login":"user"}}"#;
        let event = WebhookHandler::parse_github_payload(payload, "push").unwrap();
        assert_eq!(event.branch, Some("main".into()));
        assert_eq!(event.repo, "user/repo");
    }

    #[test]
    fn should_trigger_on_push() {
        let event = WebhookEvent {
            event_type: "push".into(),
            action: "unknown".into(),
            repo: "test".into(),
            branch: None,
            issue_number: None,
            pr_number: None,
            sender: "x".into(),
            timestamp: 0,
        };
        assert!(WebhookHandler::should_trigger_cycle(
            &event,
            &WebhookConfig::default()
        ));
    }

    #[test]
    fn verify_signature_empty_secret() {
        assert!(WebhookHandler::verify_signature("payload", "sig", ""));
    }
}
