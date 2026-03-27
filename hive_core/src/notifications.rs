//! Integración Slack/Discord para notificaciones.
//! Reporta cuando algo requiere atención humana.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationMessage {
    pub channel: String,
    pub title: String,
    pub body: String,
    pub severity: NotificationSeverity,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NotificationSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone)]
pub struct NotifierConfig {
    pub slack_webhook_url: Option<String>,
    pub discord_webhook_url: Option<String>,
    pub min_severity: NotificationSeverity,
}

impl Default for NotifierConfig {
    fn default() -> Self {
        Self {
            slack_webhook_url: std::env::var("SLACK_WEBHOOK_URL").ok(),
            discord_webhook_url: std::env::var("DISCORD_WEBHOOK_URL").ok(),
            min_severity: NotificationSeverity::Warning,
        }
    }
}

pub struct Notifier {
    config: NotifierConfig,
    history: Vec<NotificationMessage>,
}

impl Notifier {
    pub fn new(config: NotifierConfig) -> Self {
        Self {
            config,
            history: Vec::new(),
        }
    }

    pub fn notify(&mut self, title: &str, body: &str, severity: NotificationSeverity) {
        if severity_severity_rank(&severity) < severity_severity_rank(&self.config.min_severity) {
            return;
        }

        let msg = NotificationMessage {
            channel: "hive".into(),
            title: title.into(),
            body: body.into(),
            severity,
            timestamp: chrono::Utc::now().timestamp(),
        };

        // Slack
        if let Some(ref url) = self.config.slack_webhook_url {
            let _ = self.send_slack(url, &msg);
        }

        // Discord
        if let Some(ref url) = self.config.discord_webhook_url {
            let _ = self.send_discord(url, &msg);
        }

        self.history.push(msg);
    }

    fn send_slack(&self, url: &str, msg: &NotificationMessage) -> Result<(), String> {
        let payload = serde_json::json!({
            "text": format!("*{}*\n{}", msg.title, msg.body)
        });
        // En producción haría HTTP POST
        let _ = (url, payload);
        Ok(())
    }

    fn send_discord(&self, url: &str, msg: &NotificationMessage) -> Result<(), String> {
        let payload = serde_json::json!({
            "content": format!("**{}**\n{}", msg.title, msg.body)
        });
        let _ = (url, payload);
        Ok(())
    }

    pub fn history(&self) -> &[NotificationMessage] {
        &self.history
    }
}

fn severity_severity_rank(s: &NotificationSeverity) -> u8 {
    match s {
        NotificationSeverity::Info => 0,
        NotificationSeverity::Warning => 1,
        NotificationSeverity::Error => 2,
        NotificationSeverity::Critical => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifier_records_history() {
        let mut n = Notifier::new(NotifierConfig {
            min_severity: NotificationSeverity::Info,
            ..Default::default()
        });
        n.notify("Test", "Body", NotificationSeverity::Warning);
        assert_eq!(n.history().len(), 1);
    }

    #[test]
    fn notifier_filters_by_severity() {
        let mut n = Notifier::new(NotifierConfig {
            min_severity: NotificationSeverity::Error,
            ..Default::default()
        });
        n.notify("Test", "Body", NotificationSeverity::Info);
        assert!(n.history().is_empty());
    }
}
