//! Graceful shutdown del sistema.
//! Maneja SIGTERM/SIGINT para terminar workers limpiamente.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ShutdownState {
    Running,
    Draining,
    Shutdown,
}

pub struct ShutdownHandler {
    state: std::sync::Mutex<ShutdownState>,
}

impl ShutdownHandler {
    pub fn new() -> Self {
        Self {
            state: std::sync::Mutex::new(ShutdownState::Running),
        }
    }

    pub fn request_shutdown(&self) {
        if let Ok(mut s) = self.state.lock() {
            *s = ShutdownState::Draining;
        }
    }

    pub fn complete_shutdown(&self) {
        if let Ok(mut s) = self.state.lock() {
            *s = ShutdownState::Shutdown;
        }
    }

    pub fn state(&self) -> ShutdownState {
        self.state
            .lock()
            .map(|s| s.clone())
            .unwrap_or(ShutdownState::Shutdown)
    }

    pub fn should_stop(&self) -> bool {
        self.state() != ShutdownState::Running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_handler_works() {
        let h = ShutdownHandler::new();
        assert_eq!(h.state(), ShutdownState::Running);
        h.request_shutdown();
        assert!(h.should_stop());
        h.complete_shutdown();
        assert_eq!(h.state(), ShutdownState::Shutdown);
    }
}
