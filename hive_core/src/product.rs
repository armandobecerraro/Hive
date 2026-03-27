//! CLI/TUI producto mínimo — experiencia unificada para el usuario.
//!
//! Este módulo implementa la interfaz de usuario que unifica todos los
//! módulos del sistema en un flujo coherente. Es lo que convierte a Hive
//! de "motor" a "producto".
//!
//! Funcionalidades:
//! - Dashboard en tiempo real con progreso
//! - Selección interactiva de modelo
//! - Visualización de coste por ciclo
//! - Logs estructurados con niveles
//! - Resumen ejecutivo al final de cada ciclo

use serde::{Deserialize, Serialize};

/// Estado del producto.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductState {
    pub version: String,
    pub current_cycle: usize,
    pub active_model: String,
    pub active_provider: String,
    pub total_cost_usd: f64,
    pub total_tokens: usize,
    pub success_rate: f64,
    pub last_action: String,
}

/// Evento de UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiEvent {
    pub event_type: UiEventType,
    pub message: String,
    pub progress: Option<u8>,
    pub data: Option<serde_json::Value>,
}

/// Tipo de evento UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiEventType {
    CycleStarted,
    StageProgress,
    StageComplete,
    StageFailed,
    CostUpdate,
    ModelSelected,
    FileChanged,
    TestResult,
    CycleComplete,
    Error,
}

/// Producto CLI/TUI.
pub struct Product {
    state: ProductState,
    events: Vec<UiEvent>,
    output_buffer: String,
}

impl Product {
    pub fn new(version: &str) -> Self {
        Self {
            state: ProductState {
                version: version.into(),
                current_cycle: 0,
                active_model: String::new(),
                active_provider: String::new(),
                total_cost_usd: 0.0,
                total_tokens: 0,
                success_rate: 0.0,
                last_action: String::new(),
            },
            events: Vec::new(),
            output_buffer: String::new(),
        }
    }

    /// Emite un evento de UI.
    pub fn emit(&mut self, event: UiEvent) {
        self.events.push(event);
    }

    /// Actualiza el estado del modelo activo.
    pub fn set_model(&mut self, provider: &str, model: &str) {
        self.state.active_provider = provider.into();
        self.state.active_model = model.into();
        self.emit(UiEvent {
            event_type: UiEventType::ModelSelected,
            message: format!("Modelo: {provider}/{model}"),
            progress: None,
            data: None,
        });
    }

    /// Inicia un nuevo ciclo.
    pub fn start_cycle(&mut self) {
        self.state.current_cycle += 1;
        self.emit(UiEvent {
            event_type: UiEventType::CycleStarted,
            message: format!("=== Ciclo #{} ===", self.state.current_cycle),
            progress: Some(0),
            data: None,
        });
    }

    /// Reporta progreso de una etapa.
    pub fn stage_progress(&mut self, stage: &str, percent: u8) {
        self.emit(UiEvent {
            event_type: UiEventType::StageProgress,
            message: format!("[{percent:>3}%] {stage}"),
            progress: Some(percent),
            data: None,
        });
    }

    /// Reporta etapa completada.
    pub fn stage_complete(&mut self, stage: &str, duration_ms: u64) {
        self.emit(UiEvent {
            event_type: UiEventType::StageComplete,
            message: format!("  ✓ {stage} ({duration_ms}ms)"),
            progress: None,
            data: None,
        });
    }

    /// Reporta error en etapa.
    pub fn stage_failed(&mut self, stage: &str, error: &str) {
        self.emit(UiEvent {
            event_type: UiEventType::StageFailed,
            message: format!("  ✗ {stage}: {error}"),
            progress: None,
            data: None,
        });
    }

    /// Actualiza coste acumulado.
    pub fn update_cost(&mut self, tokens: usize, cost: f64) {
        self.state.total_tokens += tokens;
        self.state.total_cost_usd += cost;
        self.emit(UiEvent {
            event_type: UiEventType::CostUpdate,
            message: format!("  💰 Tokens: {} | Coste: ${:.4}", self.state.total_tokens, self.state.total_cost_usd),
            progress: None,
            data: Some(serde_json::json!({"tokens": self.state.total_tokens, "cost": self.state.total_cost_usd})),
        });
    }

    /// Reporta cambio de archivo.
    pub fn file_changed(&mut self, path: &str, action: &str) {
        self.emit(UiEvent {
            event_type: UiEventType::FileChanged,
            message: format!("  📄 {action}: {path}"),
            progress: None,
            data: None,
        });
    }

    /// Reporta resultado de tests.
    pub fn test_result(&mut self, passed: bool, details: &str) {
        let icon = if passed { "✅" } else { "❌" };
        self.emit(UiEvent {
            event_type: UiEventType::TestResult,
            message: format!("  {icon} Tests: {details}"),
            progress: None,
            data: Some(serde_json::json!({"passed": passed})),
        });
    }

    /// Finaliza el ciclo y genera resumen ejecutivo.
    pub fn complete_cycle(&mut self, success: bool, files_changed: usize, commit: Option<&str>) {
        let icon = if success { "✅" } else { "❌" };
        self.state.last_action = if success {
            "ciclo completado".into()
        } else {
            "ciclo fallido".into()
        };

        self.emit(UiEvent {
            event_type: UiEventType::CycleComplete,
            message: format!(
                "\n{icon} Ciclo #{} completado\n   Archivos: {files_changed} | Commit: {}\n   Tokens totales: {} | Coste total: ${:.4}\n",
                self.state.current_cycle,
                commit.unwrap_or("ninguno"),
                self.state.total_tokens,
                self.state.total_cost_usd,
            ),
            progress: Some(100),
            data: Some(serde_json::json!({
                "success": success,
                "files_changed": files_changed,
                "commit": commit,
                "cycle": self.state.current_cycle,
            })),
        });
    }

    /// Genera la salida formateada para el terminal.
    pub fn render(&mut self) -> String {
        let mut output = String::new();

        // Header
        output.push_str(&format!("🐝 The Hive v{}\n", self.state.version));
        output.push_str(&format!(
            "   Modelo: {}/{} | Ciclo: #{}\n",
            self.state.active_provider, self.state.active_model, self.state.current_cycle
        ));
        output.push_str(&"─".repeat(60));
        output.push('\n');

        // Eventos
        for event in &self.events {
            output.push_str(&event.message);
            output.push('\n');
        }

        // Footer
        output.push_str(&"─".repeat(60));
        output.push('\n');
        output.push_str(&format!(
            "💰 Total: {} tokens | ${:.4}\n",
            self.state.total_tokens, self.state.total_cost_usd
        ));

        self.output_buffer = output.clone();
        self.events.clear();
        output
    }

    /// Obtiene el estado actual.
    pub fn state(&self) -> &ProductState {
        &self.state
    }

    /// Obtiene la salida acumulada.
    pub fn output(&self) -> &str {
        &self.output_buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_lifecycle() {
        let mut p = Product::new("0.1.0");
        p.set_model("openai", "gpt-4o");
        p.start_cycle();
        p.stage_progress("scan_context", 25);
        p.stage_complete("scan_context", 150);
        p.update_cost(500, 0.005);
        p.file_changed("src/main.rs", "modified");
        p.test_result(true, "42 passed");
        p.complete_cycle(true, 3, Some("abc123"));

        let output = p.render();
        assert!(output.contains("The Hive v0.1.0"));
        assert!(output.contains("gpt-4o"));
        assert!(output.contains("✅"));
        assert!(output.contains("abc123"));
    }

    #[test]
    fn product_state_tracks_cost() {
        let mut p = Product::new("0.1.0");
        p.update_cost(100, 0.001);
        p.update_cost(200, 0.002);
        assert_eq!(p.state().total_tokens, 300);
        assert!((p.state().total_cost_usd - 0.003).abs() < 0.0001);
    }

    #[test]
    fn product_failure_report() {
        let mut p = Product::new("0.1.0");
        p.start_cycle();
        p.stage_failed("validate", "compilation error");
        p.complete_cycle(false, 0, None);
        let output = p.render();
        assert!(output.contains("❌"));
    }
}
