//! Retry con cambio de estrategia.
//!
//! Cuando el Consejo rechaza, no solo reintenta el mismo código.
//! Cambia de estrategia: otro modelo, más contexto, distinto approach.

use serde::{Deserialize, Serialize};

/// Estrategia de retry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RetryStrategy {
    SameModel,
    DifferentModel,
    MoreContext,
    DifferentApproach,
    HumanIntervention,
    SimplifyTask,
}

/// Estado de un retry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryState {
    pub attempt: usize,
    pub max_attempts: usize,
    pub current_strategy: RetryStrategy,
    pub previous_strategies: Vec<RetryStrategy>,
    pub last_error: Option<String>,
}

/// Manager de retries estratégicos.
pub struct RetryManager;

impl RetryManager {
    /// Selecciona la siguiente estrategia de retry.
    pub fn next_strategy(state: &RetryState) -> RetryStrategy {
        let used = &state.previous_strategies;

        // Secuencia de estrategias en orden de preferencia
        if !used.contains(&RetryStrategy::MoreContext) {
            RetryStrategy::MoreContext
        } else if !used.contains(&RetryStrategy::DifferentModel) {
            RetryStrategy::DifferentModel
        } else if !used.contains(&RetryStrategy::DifferentApproach) {
            RetryStrategy::DifferentApproach
        } else if !used.contains(&RetryStrategy::SimplifyTask) {
            RetryStrategy::SimplifyTask
        } else if state.attempt >= state.max_attempts {
            RetryStrategy::HumanIntervention
        } else {
            RetryStrategy::SameModel
        }
    }

    /// Crea un estado inicial.
    pub fn initial_state(max_attempts: usize) -> RetryState {
        RetryState {
            attempt: 0,
            max_attempts,
            current_strategy: RetryStrategy::SameModel,
            previous_strategies: Vec::new(),
            last_error: None,
        }
    }

    /// Avanza al siguiente retry.
    pub fn advance(state: &mut RetryState, error: &str) -> RetryStrategy {
        state
            .previous_strategies
            .push(state.current_strategy.clone());
        state.attempt += 1;
        state.last_error = Some(error.to_string());
        state.current_strategy = Self::next_strategy(state);
        state.current_strategy.clone()
    }

    /// Verifica si se debe reintentar.
    pub fn should_retry(state: &RetryState) -> bool {
        state.attempt < state.max_attempts
            && state.current_strategy != RetryStrategy::HumanIntervention
    }

    /// Genera un prompt modificado según la estrategia.
    pub fn modify_prompt(original: &str, strategy: &RetryStrategy, error: &str) -> String {
        match strategy {
            RetryStrategy::MoreContext => {
                format!("Intento anterior falló con error: {error}\n\nCon este contexto adicional, genera una solución diferente:\n\n{original}")
            }
            RetryStrategy::DifferentApproach => {
                format!("El approach anterior no funcionó: {error}\n\nGenera una solución usando un approach completamente diferente:\n\n{original}")
            }
            RetryStrategy::SimplifyTask => {
                format!("La solución fue demasiado compleja: {error}\n\nGenera una solución más simple y directa:\n\n{original}")
            }
            RetryStrategy::HumanIntervention => {
                format!("Se agotaron los reintentos. Último error: {error}\n\nSe requiere intervención humana para: {original}")
            }
            _ => original.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_strategy_follows_sequence() {
        let state = RetryManager::initial_state(5);
        let next = RetryManager::next_strategy(&state);
        assert!(matches!(next, RetryStrategy::MoreContext));
    }

    #[test]
    fn advance_increments_attempt() {
        let mut state = RetryManager::initial_state(5);
        RetryManager::advance(&mut state, "test error");
        assert_eq!(state.attempt, 1);
        assert_eq!(state.previous_strategies.len(), 1);
    }

    #[test]
    fn should_retry_respects_max() {
        let mut state = RetryManager::initial_state(2);
        assert!(RetryManager::should_retry(&state));
        RetryManager::advance(&mut state, "e");
        RetryManager::advance(&mut state, "e");
        assert!(!RetryManager::should_retry(&state));
    }

    #[test]
    fn modify_prompt_adds_context() {
        let modified =
            RetryManager::modify_prompt("fix this", &RetryStrategy::MoreContext, "error");
        assert!(modified.contains("error"));
        assert!(modified.contains("fix this"));
    }
}
