//! # Hive Tracing - Observabilidad Distribuida
//!
//! Sistema de tracing para monitorear el flujo completo del enjambre:
//! qué hizo cada obrera, cuánto tiempo tomó, y por qué se tomaron decisiones.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Tipo de span (operación)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpanKind {
    /// Orquestación general
    Orchestrator,
    /// Trabajo de una obrera
    Worker,
    /// Revisión del Consejo
    Council,
    /// Operación Git
    Git,
    /// Llamada al LLM
    LLM,
    /// Análisis de código
    Analysis,
}

impl SpanKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Orchestrator => "Orchestrator",
            Self::Worker => "Worker",
            Self::Council => "Council",
            Self::Git => "Git",
            Self::LLM => "LLM",
            Self::Analysis => "Analysis",
        }
    }
}

/// Estado de un span
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanStatus {
    Started,
    Completed,
    Failed,
    Cancelled,
}

/// Un span de tracing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpan {
    pub trace_id: Uuid,
    pub span_id: Uuid,
    pub parent_span_id: Option<Uuid>,
    pub kind: SpanKind,
    pub operation: String,
    pub status: SpanStatus,
    pub attributes: HashMap<String, String>,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

impl TraceSpan {
    pub fn new(trace_id: Uuid, kind: SpanKind, operation: String) -> Self {
        Self {
            trace_id,
            span_id: Uuid::new_v4(),
            parent_span_id: None,
            kind,
            operation,
            status: SpanStatus::Started,
            attributes: HashMap::new(),
            start_time: chrono::Utc::now().timestamp_millis(),
            end_time: None,
            duration_ms: None,
            error: None,
        }
    }

    pub fn with_parent(mut self, parent_id: Uuid) -> Self {
        self.parent_span_id = Some(parent_id);
        self
    }

    pub fn add_attribute(&mut self, key: String, value: String) {
        self.attributes.insert(key, value);
    }

    pub fn complete(&mut self) {
        let now = chrono::Utc::now().timestamp_millis();
        self.end_time = Some(now);
        self.duration_ms = Some((now - self.start_time) as u64);
        self.status = SpanStatus::Completed;
    }

    pub fn fail(&mut self, error: String) {
        let now = chrono::Utc::now().timestamp_millis();
        self.end_time = Some(now);
        self.duration_ms = Some((now - self.start_time) as u64);
        self.status = SpanStatus::Failed;
        self.error = Some(error);
    }
}

/// Trace completo (conjunto de spans relacionados)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub trace_id: Uuid,
    pub name: String,
    pub spans: Vec<TraceSpan>,
    pub start_time: i64,
    pub end_time: Option<i64>,
}

impl Trace {
    pub fn new(name: String) -> Self {
        Self {
            trace_id: Uuid::new_v4(),
            name,
            spans: Vec::new(),
            start_time: chrono::Utc::now().timestamp_millis(),
            end_time: None,
        }
    }

    pub fn total_duration_ms(&self) -> Option<u64> {
        self.end_time.map(|end| (end - self.start_time) as u64)
    }

    pub fn span_count(&self) -> usize {
        self.spans.len()
    }

    pub fn failed_spans(&self) -> Vec<&TraceSpan> {
        self.spans
            .iter()
            .filter(|s| s.status == SpanStatus::Failed)
            .collect()
    }
}

/// Almacén de traces
pub struct TraceStore {
    traces: Arc<RwLock<HashMap<Uuid, Trace>>>,
    max_traces: usize,
}

impl TraceStore {
    pub fn new(max_traces: usize) -> Self {
        Self {
            traces: Arc::new(RwLock::new(HashMap::new())),
            max_traces,
        }
    }

    /// Inicia un nuevo trace
    pub async fn start_trace(&self, name: String) -> Uuid {
        let trace = Trace::new(name);
        let trace_id = trace.trace_id;
        let mut traces = self.traces.write().await;
        traces.insert(trace_id, trace);

        // Limpiar traces antiguos si excedemos el máximo
        if traces.len() > self.max_traces {
            let oldest = traces.keys().copied().collect::<Vec<_>>();
            for key in oldest.iter().take(traces.len() - self.max_traces) {
                traces.remove(key);
            }
        }

        trace_id
    }

    /// Agrega un span a un trace
    pub async fn add_span(&self, trace_id: Uuid, span: TraceSpan) {
        let mut traces = self.traces.write().await;
        if let Some(trace) = traces.get_mut(&trace_id) {
            trace.spans.push(span);
        }
    }

    /// Completa un trace
    pub async fn complete_trace(&self, trace_id: Uuid) {
        let mut traces = self.traces.write().await;
        if let Some(trace) = traces.get_mut(&trace_id) {
            trace.end_time = Some(chrono::Utc::now().timestamp_millis());
        }
    }

    /// Obtiene un trace
    pub async fn get_trace(&self, trace_id: Uuid) -> Option<Trace> {
        self.traces.read().await.get(&trace_id).cloned()
    }

    /// Lista traces recientes
    pub async fn list_recent(&self, limit: usize) -> Vec<Trace> {
        let traces = self.traces.read().await;
        let mut sorted: Vec<Trace> = traces.values().cloned().collect();
        sorted.sort_by(|a, b| b.start_time.cmp(&a.start_time));
        sorted.truncate(limit);
        sorted
    }

    /// Estadísticas
    pub async fn stats(&self) -> TraceStats {
        let traces = self.traces.read().await;
        let total_spans: usize = traces.values().map(|t| t.spans.len()).sum();
        let failed_spans: usize = traces.values().map(|t| t.failed_spans().len()).sum();

        TraceStats {
            total_traces: traces.len(),
            total_spans,
            failed_spans,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TraceStats {
    pub total_traces: usize,
    pub total_spans: usize,
    pub failed_spans: usize,
}

impl Default for TraceStore {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_lifecycle() {
        let trace_id = Uuid::new_v4();
        let mut span = TraceSpan::new(trace_id, SpanKind::Worker, "generate_code".into());

        assert_eq!(span.status, SpanStatus::Started);
        assert!(span.duration_ms.is_none());

        std::thread::sleep(std::time::Duration::from_millis(10));
        span.complete();

        assert_eq!(span.status, SpanStatus::Completed);
        assert!(span.duration_ms.is_some());
        assert!(span.duration_ms.unwrap() >= 10);
    }

    #[test]
    fn test_span_failure() {
        let trace_id = Uuid::new_v4();
        let mut span = TraceSpan::new(trace_id, SpanKind::Worker, "test".into());

        std::thread::sleep(std::time::Duration::from_millis(5));
        span.fail("Timeout".into());

        assert_eq!(span.status, SpanStatus::Failed);
        assert_eq!(span.error.as_deref(), Some("Timeout"));
    }

    #[test]
    fn test_span_with_parent() {
        let trace_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();

        let span =
            TraceSpan::new(trace_id, SpanKind::Worker, "child".into()).with_parent(parent_id);

        assert_eq!(span.parent_span_id, Some(parent_id));
    }

    #[test]
    fn test_trace_aggregation() {
        let mut trace = Trace::new("full_workflow".into());

        for i in 0..5 {
            let mut span = TraceSpan::new(trace.trace_id, SpanKind::Worker, format!("step_{i}"));
            span.complete();
            trace.spans.push(span);
        }

        assert_eq!(trace.span_count(), 5);
        assert!(trace.failed_spans().is_empty());
    }

    #[test]
    fn test_trace_failed_spans() {
        let mut trace = Trace::new("test".into());

        let mut span1 = TraceSpan::new(trace.trace_id, SpanKind::Worker, "ok".into());
        span1.complete();
        trace.spans.push(span1);

        let mut span2 = TraceSpan::new(trace.trace_id, SpanKind::Worker, "fail".into());
        span2.fail("Error".into());
        trace.spans.push(span2);

        assert_eq!(trace.failed_spans().len(), 1);
    }

    #[tokio::test]
    async fn test_trace_store_lifecycle() {
        let store = TraceStore::new(100);

        let trace_id = store.start_trace("test_workflow".into()).await;

        let span = TraceSpan::new(trace_id, SpanKind::Worker, "step1".into());
        store.add_span(trace_id, span).await;

        store.complete_trace(trace_id).await;

        let trace = store.get_trace(trace_id).await.unwrap();
        assert_eq!(trace.name, "test_workflow");
        assert_eq!(trace.span_count(), 1);
        assert!(trace.end_time.is_some());
    }

    #[tokio::test]
    async fn test_trace_store_list_recent() {
        let store = TraceStore::new(100);

        for i in 0..5 {
            store.start_trace(format!("trace_{i}")).await;
        }

        let recent = store.list_recent(3).await;
        assert_eq!(recent.len(), 3);
    }

    #[tokio::test]
    async fn test_trace_store_stats() {
        let store = TraceStore::new(100);

        let t1 = store.start_trace("t1".into()).await;
        let t2 = store.start_trace("t2".into()).await;

        store
            .add_span(t1, TraceSpan::new(t1, SpanKind::Worker, "s1".into()))
            .await;
        store
            .add_span(t1, TraceSpan::new(t1, SpanKind::Worker, "s2".into()))
            .await;
        store
            .add_span(t2, TraceSpan::new(t2, SpanKind::Worker, "s3".into()))
            .await;

        let stats = store.stats().await;
        assert_eq!(stats.total_traces, 2);
        assert_eq!(stats.total_spans, 3);
    }

    #[tokio::test]
    async fn test_trace_store_max_limit() {
        let store = TraceStore::new(3);

        for i in 0..5 {
            store.start_trace(format!("trace_{i}")).await;
        }

        let recent = store.list_recent(10).await;
        assert_eq!(recent.len(), 3, "Solo debe haber 3 traces (max_traces)");
    }

    #[test]
    fn test_span_attributes() {
        let trace_id = Uuid::new_v4();
        let mut span = TraceSpan::new(trace_id, SpanKind::LLM, "call".into());

        span.add_attribute("model".into(), "gpt-4".into());
        span.add_attribute("tokens".into(), "1500".into());

        assert_eq!(span.attributes.get("model").unwrap(), "gpt-4");
    }

    #[test]
    fn test_span_kind_variants() {
        assert_eq!(SpanKind::Orchestrator.as_str(), "Orchestrator");
        assert_eq!(SpanKind::Worker.as_str(), "Worker");
        assert_eq!(SpanKind::Council.as_str(), "Council");
        assert_eq!(SpanKind::Git.as_str(), "Git");
        assert_eq!(SpanKind::LLM.as_str(), "LLM");
        assert_eq!(SpanKind::Analysis.as_str(), "Analysis");
    }

    #[test]
    fn test_span_status_variants() {
        let started = SpanStatus::Started;
        let completed = SpanStatus::Completed;
        let failed = SpanStatus::Failed;
        let cancelled = SpanStatus::Cancelled;

        assert_eq!(started, SpanStatus::Started);
        assert_eq!(completed, SpanStatus::Completed);
        assert_eq!(failed, SpanStatus::Failed);
        assert_eq!(cancelled, SpanStatus::Cancelled);
    }

    #[test]
    fn test_trace_total_duration() {
        let mut trace = Trace::new("test".into());
        assert!(trace.total_duration_ms().is_none());

        trace.end_time = Some(trace.start_time + 1000);
        assert_eq!(trace.total_duration_ms(), Some(1000));
    }

    #[test]
    fn test_trace_span_serialization() {
        let trace_id = Uuid::new_v4();
        let span = TraceSpan::new(trace_id, SpanKind::Worker, "test".into());

        let serialized = serde_json::to_string(&span).unwrap();
        let deserialized: TraceSpan = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.operation, "test");
        assert_eq!(deserialized.kind, SpanKind::Worker);
    }

    #[test]
    fn test_trace_serialization() {
        let mut trace = Trace::new("test".into());
        let span = TraceSpan::new(trace.trace_id, SpanKind::Worker, "step".into());
        trace.spans.push(span);

        let serialized = serde_json::to_string(&trace).unwrap();
        let deserialized: Trace = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.name, "test");
        assert_eq!(deserialized.span_count(), 1);
    }

    #[test]
    fn test_trace_stats_default() {
        let stats = TraceStats {
            total_traces: 10,
            total_spans: 50,
            failed_spans: 2,
        };
        assert_eq!(stats.total_traces, 10);
        assert_eq!(stats.total_spans, 50);
        assert_eq!(stats.failed_spans, 2);
    }

    #[tokio::test]
    async fn test_trace_store_get_nonexistent() {
        let store = TraceStore::new(100);
        let trace = store.get_trace(Uuid::new_v4()).await;
        assert!(trace.is_none());
    }

    #[tokio::test]
    async fn test_trace_store_add_span_to_nonexistent() {
        let store = TraceStore::new(100);
        let span = TraceSpan::new(Uuid::new_v4(), SpanKind::Worker, "test".into());
        store.add_span(Uuid::new_v4(), span).await;
        let stats = store.stats().await;
        assert_eq!(stats.total_traces, 0);
    }

    #[test]
    fn test_trace_span_debug() {
        let trace_id = Uuid::new_v4();
        let span = TraceSpan::new(trace_id, SpanKind::LLM, "call".into());
        let debug_str = format!("{:?}", span);
        assert!(debug_str.contains("LLM"));
    }

    #[test]
    fn test_trace_debug() {
        let trace = Trace::new("test".into());
        let debug_str = format!("{:?}", trace);
        assert!(debug_str.contains("test"));
    }
}
