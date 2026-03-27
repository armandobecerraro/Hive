//! Cost tracking de uso de LLM.
//!
//! Rastrea tokens consumidos, costo por ciclo, costo por merge aprobado.
//! Inspirado en LangSmith y Langfuse que ofrecen observabilidad de costos.

use serde::{Deserialize, Serialize};

/// Registro de uso de un LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub timestamp: i64,
    pub model: String,
    pub provider: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cost_usd: f64,
    pub task_description: String,
    pub cycle_id: String,
}

/// Estadísticas de uso.
#[derive(Debug, Clone, Serialize, Default)]
pub struct UsageStats {
    pub total_input_tokens: usize,
    pub total_output_tokens: usize,
    pub total_cost_usd: f64,
    pub total_calls: usize,
    pub cost_per_merge: Option<f64>,
    pub cost_per_cycle: f64,
}

/// Tracker de costos.
pub struct CostTracker {
    records: Vec<UsageRecord>,
    current_cycle_id: String,
}

impl CostTracker {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            current_cycle_id: uuid::Uuid::new_v4().to_string(),
        }
    }

    /// Registra uso de LLM.
    pub fn record(
        &mut self,
        model: &str,
        provider: &str,
        input_tokens: usize,
        output_tokens: usize,
        cost_per_1k_input: f64,
        cost_per_1k_output: f64,
        task: &str,
    ) {
        let cost = (input_tokens as f64 / 1000.0) * cost_per_1k_input
            + (output_tokens as f64 / 1000.0) * cost_per_1k_output;
        self.records.push(UsageRecord {
            timestamp: chrono::Utc::now().timestamp(),
            model: model.to_string(),
            provider: provider.to_string(),
            input_tokens,
            output_tokens,
            cost_usd: cost,
            task_description: task.to_string(),
            cycle_id: self.current_cycle_id.clone(),
        });
    }

    /// Inicia un nuevo ciclo.
    pub fn new_cycle(&mut self) {
        self.current_cycle_id = uuid::Uuid::new_v4().to_string();
    }

    /// Calcula estadísticas totales.
    pub fn stats(&self) -> UsageStats {
        let total_input: usize = self.records.iter().map(|r| r.input_tokens).sum();
        let total_output: usize = self.records.iter().map(|r| r.output_tokens).sum();
        let total_cost: f64 = self.records.iter().map(|r| r.cost_usd).sum();

        UsageStats {
            total_input_tokens: total_input,
            total_output_tokens: total_output,
            total_cost_usd: total_cost,
            total_calls: self.records.len(),
            cost_per_merge: None,
            cost_per_cycle: total_cost,
        }
    }

    /// Estadísticas por modelo.
    pub fn stats_by_model(&self) -> std::collections::HashMap<String, UsageStats> {
        let mut map: std::collections::HashMap<String, UsageStats> =
            std::collections::HashMap::new();
        for record in &self.records {
            let entry = map.entry(record.model.clone()).or_default();
            entry.total_input_tokens += record.input_tokens;
            entry.total_output_tokens += record.output_tokens;
            entry.total_cost_usd += record.cost_usd;
            entry.total_calls += 1;
        }
        map
    }

    /// Exporta registros como JSON.
    pub fn export_json(&self) -> String {
        serde_json::to_string_pretty(&self.records).unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_tracker_records() {
        let mut tracker = CostTracker::new();
        tracker.record("gpt-4o", "openai", 1000, 500, 0.005, 0.015, "test task");
        assert_eq!(tracker.len(), 1);
        let stats = tracker.stats();
        assert_eq!(stats.total_calls, 1);
        assert!(stats.total_cost_usd > 0.0);
    }

    #[test]
    fn cost_tracker_stats_by_model() {
        let mut tracker = CostTracker::new();
        tracker.record("gpt-4o", "openai", 100, 50, 0.005, 0.015, "t1");
        tracker.record("codellama", "ollama", 100, 50, 0.0, 0.0, "t2");
        let by_model = tracker.stats_by_model();
        assert_eq!(by_model.len(), 2);
    }
}
