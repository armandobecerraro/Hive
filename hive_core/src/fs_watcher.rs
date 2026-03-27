//! File system watcher para detectar cambios en tiempo real.
//!
//! Monitorea el repositorio y reacciona a cambios del usuario mientras trabaja.
//! Usa el crate `notify` para eventos del filesystem.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Evento del filesystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsEvent {
    pub event_type: FsEventType,
    pub path: String,
    pub timestamp: i64,
}

/// Tipo de evento.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FsEventType {
    Created,
    Modified,
    Deleted,
    Renamed,
}

/// Configuración del watcher.
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    pub watch_paths: Vec<PathBuf>,
    pub ignore_patterns: Vec<String>,
    pub debounce_ms: u64,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            watch_paths: vec![],
            ignore_patterns: vec![
                ".git".into(),
                "target".into(),
                "node_modules".into(),
                ".hive".into(),
                "__pycache__".into(),
            ],
            debounce_ms: 500,
        }
    }
}

/// Cola de eventos del filesystem.
pub struct FsEventQueue {
    events: std::sync::Mutex<Vec<FsEvent>>,
    config: WatcherConfig,
}

impl FsEventQueue {
    pub fn new(config: WatcherConfig) -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
            config,
        }
    }

    /// Registra un evento.
    pub fn push(&self, event_type: FsEventType, path: &str) {
        // Filtrar patrones ignorados
        for pattern in &self.config.ignore_patterns {
            if path.contains(pattern) {
                return;
            }
        }

        let event = FsEvent {
            event_type,
            path: path.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        };

        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }

    /// Obtiene y limpia eventos acumulados.
    pub fn drain(&self) -> Vec<FsEvent> {
        if let Ok(mut events) = self.events.lock() {
            std::mem::take(&mut *events)
        } else {
            Vec::new()
        }
    }

    /// Verifica si hay eventos pendientes.
    pub fn has_events(&self) -> bool {
        self.events.lock().map(|e| !e.is_empty()).unwrap_or(false)
    }

    /// Cuenta eventos pendientes.
    pub fn pending_count(&self) -> usize {
        self.events.lock().map(|e| e.len()).unwrap_or(0)
    }

    /// Filtra eventos por tipo.
    pub fn drain_by_type(&self, event_type: &FsEventType) -> Vec<FsEvent> {
        if let Ok(mut events) = self.events.lock() {
            let (matching, remaining): (Vec<FsEvent>, Vec<FsEvent>) =
                events.drain(..).partition(|e| {
                    std::mem::discriminant(&e.event_type) == std::mem::discriminant(event_type)
                });
            *events = remaining;
            matching
        } else {
            Vec::new()
        }
    }

    /// Obtiene archivos modificados desde la última consulta.
    pub fn modified_files(&self) -> Vec<String> {
        if let Ok(events) = self.events.lock() {
            events
                .iter()
                .filter(|e| matches!(e.event_type, FsEventType::Modified))
                .map(|e| e.path.clone())
                .collect()
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_queue_push_and_drain() {
        let queue = FsEventQueue::new(WatcherConfig::default());
        queue.push(FsEventType::Created, "/repo/test.rs");
        queue.push(FsEventType::Modified, "/repo/main.rs");
        assert_eq!(queue.pending_count(), 2);

        let events = queue.drain();
        assert_eq!(events.len(), 2);
        assert!(!queue.has_events());
    }

    #[test]
    fn event_queue_ignores_git() {
        let queue = FsEventQueue::new(WatcherConfig::default());
        queue.push(FsEventType::Modified, "/repo/.git/HEAD");
        assert_eq!(queue.pending_count(), 0);
    }

    #[test]
    fn drain_by_type_filters() {
        let queue = FsEventQueue::new(WatcherConfig::default());
        queue.push(FsEventType::Created, "a.rs");
        queue.push(FsEventType::Modified, "b.rs");
        queue.push(FsEventType::Created, "c.rs");

        let created = queue.drain_by_type(&FsEventType::Created);
        assert_eq!(created.len(), 2);
        assert_eq!(queue.pending_count(), 1);
    }

    #[test]
    fn modified_files_returns_paths() {
        let queue = FsEventQueue::new(WatcherConfig::default());
        queue.push(FsEventType::Modified, "src/main.rs");
        queue.push(FsEventType::Created, "src/lib.rs");
        let modified = queue.modified_files();
        assert_eq!(modified.len(), 1);
        assert_eq!(modified[0], "src/main.rs");
    }
}
