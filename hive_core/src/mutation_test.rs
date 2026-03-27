//! Mutation testing para medir calidad real de los tests.
//! Inspirado en cargo-mutants.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationResult {
    pub mutation: String,
    pub killed: bool,
    pub test_output: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MutationReport {
    pub total_mutations: usize,
    pub killed: usize,
    pub survived: usize,
    pub score: f64,
}

pub struct MutationTester;

impl MutationTester {
    /// Genera mutaciones simples de un archivo.
    pub fn generate_mutations(content: &str) -> Vec<(String, String)> {
        let mut mutations = Vec::new();

        // Mutación 1: Reemplazar == por !=
        if content.contains("== ") {
            mutations.push(("==→!=".into(), content.replace("== ", "!= ")));
        }

        // Mutación 2: Reemplazar + por -
        if content.contains(" + ") {
            mutations.push(("+→-".into(), content.replace(" + ", " - ")));
        }

        // Mutación 3: Reemplazar true por false
        if content.contains("true") {
            mutations.push(("true→false".into(), content.replace("true", "false")));
        }

        mutations
    }

    /// Calcula mutation score.
    pub fn calculate_score(results: &[MutationResult]) -> MutationReport {
        let killed = results.iter().filter(|r| r.killed).count();
        let total = results.len();
        MutationReport {
            total_mutations: total,
            killed,
            survived: total - killed,
            score: if total > 0 {
                killed as f64 / total as f64 * 100.0
            } else {
                100.0
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_mutations_creates() {
        let mutations = MutationTester::generate_mutations("if x == 1 { return true; }");
        assert!(!mutations.is_empty());
    }

    #[test]
    fn calculate_score_works() {
        let results = vec![
            MutationResult {
                mutation: "a".into(),
                killed: true,
                test_output: "".into(),
            },
            MutationResult {
                mutation: "b".into(),
                killed: false,
                test_output: "".into(),
            },
        ];
        let report = MutationTester::calculate_score(&results);
        assert_eq!(report.score, 50.0);
    }
}
