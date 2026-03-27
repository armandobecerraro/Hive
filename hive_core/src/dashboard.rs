//! Web dashboard spec y datos para UI web.
//! Genera JSON con estado del sistema para consumo por frontend.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardState {
    pub system_status: String,
    pub active_workers: usize,
    pub total_cycles: usize,
    pub last_cycle_time: String,
    pub merge_rate: f64,
    pub recent_decisions: Vec<DecisionSummary>,
    pub worker_details: Vec<WorkerSummary>,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionSummary {
    pub time: String,
    pub decision: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSummary {
    pub id: String,
    pub specialist: String,
    pub status: String,
    pub branch: String,
}

pub struct Dashboard;

impl Dashboard {
    pub fn generate_state(
        active_workers: usize,
        total_cycles: usize,
        version: &str,
    ) -> DashboardState {
        DashboardState {
            system_status: if active_workers > 0 {
                "running".into()
            } else {
                "idle".into()
            },
            active_workers,
            total_cycles,
            last_cycle_time: chrono::Utc::now().to_rfc3339(),
            merge_rate: 0.0,
            recent_decisions: vec![],
            worker_details: vec![],
            version: version.into(),
        }
    }

    pub fn to_json(state: &DashboardState) -> String {
        serde_json::to_string_pretty(state).unwrap_or_default()
    }

    pub fn generate_html(data_json: &str) -> String {
        format!(
            r#"<!DOCTYPE html>
<html><head><title>The Hive Dashboard</title>
<style>
body {{ font-family: monospace; background: #1a1a2e; color: #e0e0e0; padding: 20px; }}
.card {{ background: #16213e; border-radius: 8px; padding: 16px; margin: 8px; display: inline-block; }}
h1 {{ color: #0f3460; }}
.status {{ color: #00ff88; }}
.worker {{ background: #0f3460; padding: 8px; margin: 4px; border-radius: 4px; }}
</style></head><body>
<h1>🐝 The Hive Dashboard</h1>
<div id="app"></div>
<script>
const data = {data_json};
document.getElementById('app').innerHTML = `
  <div class="card"><h3>Status</h3><p class="status">${{data.system_status}}</p></div>
  <div class="card"><h3>Active Workers</h3><p>${{data.active_workers}}</p></div>
  <div class="card"><h3>Total Cycles</h3><p>${{data.total_cycles}}</p></div>
  <div class="card"><h3>Version</h3><p>${{data.version}}</p></div>
`;
</script></body></html>"#
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_state_serializable() {
        let state = Dashboard::generate_state(3, 42, "0.1.0");
        let json = Dashboard::to_json(&state);
        assert!(json.contains("active_workers"));
    }

    #[test]
    fn generate_html_works() {
        let html = Dashboard::generate_html("{}");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("The Hive Dashboard"));
    }
}
