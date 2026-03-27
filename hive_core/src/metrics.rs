//! Exportación de métricas Prometheus/Grafana.
//! Workers activos, merges por hora, tasa de aprobación.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPoint {
    pub name: String,
    pub value: f64,
    pub labels: std::collections::HashMap<String, String>,
    pub timestamp: i64,
}

pub struct MetricsExporter {
    points: Vec<MetricPoint>,
}

impl MetricsExporter {
    pub fn new() -> Self {
        Self { points: Vec::new() }
    }

    pub fn record(
        &mut self,
        name: &str,
        value: f64,
        labels: std::collections::HashMap<String, String>,
    ) {
        self.points.push(MetricPoint {
            name: name.into(),
            value,
            labels,
            timestamp: chrono::Utc::now().timestamp(),
        });
    }

    pub fn export_prometheus(&self) -> String {
        let mut output = String::new();
        for point in &self.points {
            let labels: Vec<String> = point
                .labels
                .iter()
                .map(|(k, v)| format!("{k}=\"{v}\""))
                .collect();
            let label_str = if labels.is_empty() {
                String::new()
            } else {
                format!("{{{}}}", labels.join(","))
            };
            output.push_str(&format!("{}{} {}\n", point.name, label_str, point.value));
        }
        output
    }

    pub fn record_merge(&mut self, success: bool) {
        let mut labels = std::collections::HashMap::new();
        labels.insert("success".into(), success.to_string());
        self.record("hive_merges_total", 1.0, labels);
    }

    pub fn record_worker(&mut self, active: usize) {
        self.record(
            "hive_workers_active",
            active as f64,
            std::collections::HashMap::new(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_prometheus_format() {
        let mut m = MetricsExporter::new();
        m.record("test_metric", 42.0, std::collections::HashMap::new());
        let prom = m.export_prometheus();
        assert!(prom.contains("test_metric 42"));
    }
}
