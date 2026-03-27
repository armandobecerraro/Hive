//! OpenTelemetry observabilidad distribuida.
//!
//! Instrumenta el ciclo de La Reina, obreras y Consejo con traces y spans.
//! Exporta métricas a Jaeger/Zipkin/Grafana Tempo vía OTLP.

#[cfg(feature = "multiagent")]
use anyhow::Result;
#[cfg(feature = "multiagent")]
use opentelemetry::trace::{TraceContextExt, TracerProvider};
#[cfg(feature = "multiagent")]
use opentelemetry_otlp::WithExportConfig;
#[cfg(feature = "multiagent")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "multiagent")]
use tracing_subscriber::layer::SubscriberExt;
#[cfg(feature = "multiagent")]
use tracing_subscriber::util::SubscriberInitExt;

/// Configuración de telemetría.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Endpoint OTLP (default: http://localhost:4317).
    pub otlp_endpoint: String,
    /// Nombre del servicio.
    pub service_name: String,
    /// Activar exportación a OTLP.
    pub enable_otlp: bool,
}

#[cfg(feature = "multiagent")]
impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:4317".to_string()),
            service_name: "hive_core".to_string(),
            enable_otlp: std::env::var("HIVE_OTEL_ENABLED")
                .map(|v| v == "1" || v == "true")
                .unwrap_or(false),
        }
    }
}

/// Inicializa la telemetría OpenTelemetry + tracing.
#[cfg(feature = "multiagent")]
pub fn init_telemetry(config: &TelemetryConfig) -> Result<opentelemetry_sdk::trace::Tracer> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.otlp_endpoint)
        .build()
        .map_err(|e| anyhow::anyhow!("crear exporter OTLP: {e}"))?;

    let tracer_provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(opentelemetry_sdk::Resource::new(vec![
            opentelemetry::KeyValue::new("service.name", config.service_name.clone()),
            opentelemetry::KeyValue::new("service.version", "0.1.0"),
        ]))
        .build();

    let tracer = tracer_provider.tracer(config.service_name.clone());

    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer.clone());

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(telemetry_layer)
        .init();

    Ok(tracer)
}

/// Crea un span manual para una operación del enjambre.
#[cfg(feature = "multiagent")]
pub fn create_span(name: &str, attributes: Vec<(&str, String)>) -> opentelemetry::Context {
    use opentelemetry::trace::{Span, Tracer};
    let provider = opentelemetry::global::tracer_provider();
    let tracer = provider.tracer("hive_core");

    let mut span = tracer.start(name.to_string());
    for (key, value) in attributes {
        span.set_attribute(opentelemetry::KeyValue::new(key.to_string(), value));
    }

    opentelemetry::Context::current_with_span(span)
}

/// Finaliza el proveedor de traces (para shutdown limpio).
#[cfg(feature = "multiagent")]
pub fn shutdown_telemetry() {
    opentelemetry::global::shutdown_tracer_provider();
}

// Stubs sin la feature
#[cfg(not(feature = "multiagent"))]
pub struct TelemetryConfig;

#[cfg(not(feature = "multiagent"))]
impl Default for TelemetryConfig {
    fn default() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "multiagent")]
    #[test]
    fn telemetry_config_default() {
        let cfg = TelemetryConfig::default();
        assert_eq!(cfg.service_name, "hive_core");
    }
}
