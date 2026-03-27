//! Observabilidad completa — combinación de logs, metrics y traces.
//! Unifica todas las señales de observabilidad del sistema.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityEvent {
    pub timestamp: i64,
    pub level: LogLevel,
    pub component: String,
    pub message: String,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub metrics: std::collections::HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

pub struct ObservabilityCollector {
    events: Vec<ObservabilityEvent>,
    max_events: usize,
}

impl ObservabilityCollector {
    pub fn new(max_events: usize) -> Self {
        Self {
            events: Vec::new(),
            max_events,
        }
    }

    pub fn log(&mut self, level: LogLevel, component: &str, message: &str) {
        self.events.push(ObservabilityEvent {
            timestamp: chrono::Utc::now().timestamp(),
            level,
            component: component.into(),
            message: message.into(),
            trace_id: None,
            span_id: None,
            metrics: std::collections::HashMap::new(),
        });
        if self.events.len() > self.max_events {
            self.events.remove(0);
        }
    }

    pub fn log_with_metrics(
        &mut self,
        level: LogLevel,
        component: &str,
        message: &str,
        metrics: std::collections::HashMap<String, f64>,
    ) {
        self.events.push(ObservabilityEvent {
            timestamp: chrono::Utc::now().timestamp(),
            level,
            component: component.into(),
            message: message.into(),
            trace_id: None,
            span_id: None,
            metrics,
        });
        if self.events.len() > self.max_events {
            self.events.remove(0);
        }
    }

    pub fn events(&self) -> &[ObservabilityEvent] {
        &self.events
    }

    pub fn errors(&self) -> Vec<&ObservabilityEvent> {
        self.events
            .iter()
            .filter(|e| e.level == LogLevel::Error)
            .collect()
    }

    pub fn export_jsonl(&self) -> String {
        self.events
            .iter()
            .map(|e| serde_json::to_string(e).unwrap_or_default())
            .collect::<Vec<String>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_logs_events() {
        let mut c = ObservabilityCollector::new(100);
        c.log(LogLevel::Info, "orchestrator", "cycle started");
        c.log(LogLevel::Error, "agent", "task failed");
        assert_eq!(c.events().len(), 2);
        assert_eq!(c.errors().len(), 1);
    }

    #[test]
    fn export_jsonl_works() {
        let mut c = ObservabilityCollector::new(100);
        c.log(LogLevel::Info, "test", "msg");
        let jsonl = c.export_jsonl();
        assert!(jsonl.contains("test"));
    }
}
