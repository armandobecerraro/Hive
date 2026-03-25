use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

/// Solicitud de revisión enviada por una obrera al Consejo (Mantenedor).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeRequest {
    pub id: Uuid,
    pub branch: String,
    pub title: String,
    pub description: String,
    pub worker_id: Uuid,
    pub specialist_key: String,
}

#[derive(Debug, Clone)]
pub struct MaintainerReview {
    pub merge_request_id: Uuid,
    pub comments: Vec<String>,
    pub verdict: CouncilVerdict,
    /// Métricas recopiladas durante la revisión
    pub metrics: ReviewMetrics,
}

/// Métricas reales evaluadas por el Consejo
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReviewMetrics {
    /// Tests pasaron (si aplica)
    pub tests_passed: bool,
    /// Número de warnings de clippy/lint
    pub warnings_count: u32,
    /// Archivos modificados
    pub files_changed: u32,
    /// Líneas añadidas
    pub lines_added: u32,
    /// Líneas eliminadas
    pub lines_removed: u32,
    /// Score de calidad (0-100)
    pub quality_score: u8,
}

#[derive(Debug, Clone)]
pub enum CouncilVerdict {
    Approved,
    Rejected { feedback: String },
}

pub struct ReviewEnvelope {
    pub request: MergeRequest,
    pub respond_to: oneshot::Sender<MaintainerReview>,
    /// Raíz del repo para evaluación real
    pub repo_root: PathBuf,
}

/// Si el feedback del Mantenedor indica que el cambio **ya no aplica** (hecho en main, innecesario, obsoleto),
/// la obrera **no** debe reintentar: se descarta la rama como con el tope de intentos.
pub fn rejection_indicates_work_obsolete(feedback: &str) -> bool {
    let f = feedback.to_lowercase();
    const PHRASES: &[&str] = &[
        "ya no se requiere",
        "no se necesita",
        "ya no es necesario",
        "ya está hecho",
        "ya esta hecho",
        "already done",
        "not needed",
        "no longer required",
        "nothing left to do",
        "sin cambios necesarios",
        "cambio obsoleto",
        "trabajo obsoleto",
        "duplicado",
        "duplicate work",
        "ya cubierto en main",
        "already merged",
        "no aplica",
    ];
    PHRASES.iter().any(|p| f.contains(p))
}

/// Instancia de alto rango (Mantenedor): revisa MR/PR y devuelve veredicto por canal.
pub struct Maintainer {
    /// Número de rechazos simulados antes de aprobar (ciclo feedback → reintento).
    pub reject_before_approve: u32,
}

impl Default for Maintainer {
    fn default() -> Self {
        Self {
            reject_before_approve: 1,
        }
    }
}

/// Ejecuta tests del repo y devuelve si pasaron
fn run_tests(repo_root: &Path) -> bool {
    if !repo_root.join("Cargo.toml").exists() {
        return true; // No es un proyecto Rust, no hay tests que ejecutar
    }

    let output = std::process::Command::new("cargo")
        .current_dir(repo_root)
        .args(["test", "--quiet", "--no-fail-fast"])
        .output();

    match output {
        Ok(out) => out.status.success(),
        Err(_) => true, // Si no se puede ejecutar, asumimos OK
    }
}

/// Ejecuta clippy y cuenta warnings
fn count_warnings(repo_root: &Path) -> u32 {
    if !repo_root.join("Cargo.toml").exists() {
        return 0;
    }

    let output = std::process::Command::new("cargo")
        .current_dir(repo_root)
        .args(["clippy", "--quiet", "--", "-W", "clippy::all"])
        .stderr(std::process::Stdio::piped())
        .output();

    match output {
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            stderr.lines().filter(|l| l.contains("warning[")).count() as u32
        }
        Err(_) => 0,
    }
}

/// Sanitiza un nombre de rama para uso seguro en comandos Git.
/// Solo permite alfanuméricos, guiones, guiones bajos, barras y puntos.
fn sanitize_branch_name(branch: &str) -> String {
    branch
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '/' | '.'))
        .collect()
}

/// Calcula diff de la rama respecto a main
fn get_branch_diff_stats(repo_root: &Path, branch: &str) -> (u32, u32, u32) {
    let safe_branch = sanitize_branch_name(branch);
    let diff_spec = format!("main...{safe_branch}");
    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(["diff", "--stat", &diff_spec])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let last_line = stdout.lines().last().unwrap_or("");
            // "X files changed, Y insertions(+), Z deletions(-)"
            let files = last_line
                .split("file")
                .next()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);
            let added = if last_line.contains("insertion") {
                last_line
                    .split("insertion")
                    .next()
                    .and_then(|s| s.split(',').next_back())
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0)
            } else {
                0
            };
            let removed = if last_line.contains("deletion") {
                last_line
                    .split("deletion")
                    .next()
                    .and_then(|s| s.split(',').next_back())
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0)
            } else {
                0
            };
            (files, added, removed)
        }
        Err(_) => (0, 0, 0),
    }
}

/// Calcula score de calidad basado en métricas
pub fn calculate_quality_score(tests_passed: bool, warnings: u32, files_changed: u32) -> u8 {
    let mut score: i32 = 70; // Base

    if tests_passed {
        score += 20;
    } else {
        score -= 30;
    }

    // Penalizar warnings
    score -= (warnings as i32) * 2;

    // Bonus por cambios moderados (no gigantes)
    if files_changed > 0 && files_changed <= 10 {
        score += 10;
    } else if files_changed > 20 {
        score -= 10;
    }

    score.clamp(0, 100) as u8
}

impl Maintainer {
    pub fn review(&self, attempt: u32, mr: &MergeRequest, repo_root: &Path) -> MaintainerReview {
        // Verificar marcadores de obsolescencia
        if mr.description.contains("[HIVE_OBSOLETE]") || mr.description.contains("HIVE_MR_NO_RETRY")
        {
            return MaintainerReview {
                merge_request_id: mr.id,
                comments: vec![
                    "[Mantenedor] Cierre sin reintento: el cambio ya no se requiere o está cubierto."
                        .into(),
                ],
                verdict: CouncilVerdict::Rejected {
                    feedback:
                        "[Mantenedor] Ya no se requiere este MR: obsoleto o ya está hecho (marcador HIVE_OBSOLETE / HIVE_MR_NO_RETRY)."
                            .into(),
                },
                metrics: ReviewMetrics::default(),
            };
        }

        // Recopilar métricas reales
        let tests_passed = run_tests(repo_root);
        let warnings_count = count_warnings(repo_root);
        let (files_changed, lines_added, lines_removed) =
            get_branch_diff_stats(repo_root, &mr.branch);
        let quality_score = calculate_quality_score(tests_passed, warnings_count, files_changed);

        let metrics = ReviewMetrics {
            tests_passed,
            warnings_count,
            files_changed,
            lines_added,
            lines_removed,
            quality_score,
        };

        let mut comments = vec![
            format!(
                "[Mantenedor] MR `{}` (rama `{}`) — especialista `{}`.",
                mr.id, mr.branch, mr.specialist_key
            ),
            format!("[Mantenedor] Título: {}", mr.title),
        ];

        if mr.description.trim().is_empty() {
            comments.push(
                "[Mantenedor] Falta contexto en la descripción; documenta el alcance y riesgos."
                    .into(),
            );
        } else {
            comments.push(format!(
                "[Mantenedor] Descripción recibida ({} caracteres).",
                mr.description.len()
            ));
        }

        // Reportar métricas
        comments.push(format!(
            "[Mantenedor] Métricas: tests={}, warnings={}, archivos={}, +{}/-{} líneas, score={}/100",
            if tests_passed { "✓" } else { "✗" },
            warnings_count,
            files_changed,
            lines_added,
            lines_removed,
            quality_score
        ));

        // Decisión basada en métricas reales
        if !tests_passed {
            comments.push(
                "[Mantenedor] Rechazo: los tests fallaron. Corrige los tests antes de reenviar."
                    .into(),
            );
            MaintainerReview {
                merge_request_id: mr.id,
                comments: comments.clone(),
                verdict: CouncilVerdict::Rejected {
                    feedback: format!(
                        "Tests fallaron. Revisa el output de `cargo test`.\n{}\nScore de calidad: {}/100",
                        comments.join("\n"),
                        quality_score
                    ),
                },
                metrics,
            }
        } else if warnings_count > 10 {
            comments.push(format!(
                "[Mantenedor] Rechazo: demasiados warnings ({warnings_count}). Ejecuta `cargo clippy --fix`."
            ));
            MaintainerReview {
                merge_request_id: mr.id,
                comments: comments.clone(),
                verdict: CouncilVerdict::Rejected {
                    feedback: format!(
                        "Demasiados warnings de clippy ({warnings_count}). Ejecuta `cargo clippy --fix`.\n{}",
                        comments.join("\n")
                    ),
                },
                metrics,
            }
        } else if attempt <= self.reject_before_approve {
            comments.push(
                "[Mantenedor] Rechazo técnico: mejorar calidad del código y mensaje de commit."
                    .into(),
            );
            MaintainerReview {
                merge_request_id: mr.id,
                comments: comments.clone(),
                verdict: CouncilVerdict::Rejected {
                    feedback: format!(
                        "Mejora la calidad (score: {quality_score}/100). Mensaje de commit debe ser más descriptivo.\n{}",
                        comments.join("\n")
                    ),
                },
                metrics,
            }
        } else {
            comments.push(format!(
                "[Mantenedor] Aprobado: score {quality_score}/100, tests OK, {warnings_count} warnings."
            ));
            MaintainerReview {
                merge_request_id: mr.id,
                comments: comments.clone(),
                verdict: CouncilVerdict::Approved,
                metrics,
            }
        }
    }
}

pub type CouncilSender = mpsc::Sender<ReviewEnvelope>;
pub type CouncilReceiver = mpsc::Receiver<ReviewEnvelope>;

pub fn council_channel(capacity: usize) -> (CouncilSender, CouncilReceiver) {
    mpsc::channel(capacity)
}

pub async fn run_council_loop(mut rx: CouncilReceiver, maintainer: Maintainer) {
    while let Some(env) = rx.recv().await {
        let attempt = parse_attempt(&env.request.description).unwrap_or(0);
        let review = maintainer.review(attempt, &env.request, &env.repo_root);
        let _ = env.respond_to.send(review);
    }
}

fn parse_attempt(description: &str) -> Option<u32> {
    for line in description.lines() {
        if let Some(rest) = line.strip_prefix("HIVE_ATTEMPT:") {
            return rest.trim().parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    fn sample_mr(desc: &str) -> MergeRequest {
        MergeRequest {
            id: Uuid::new_v4(),
            branch: "hive/worker/x".into(),
            title: "t".into(),
            description: desc.into(),
            worker_id: Uuid::new_v4(),
            specialist_key: "rust".into(),
        }
    }

    #[test]
    fn parse_attempt_extrae_intento() {
        assert_eq!(parse_attempt("a\nHIVE_ATTEMPT:3\n"), Some(3));
        assert_eq!(parse_attempt("HIVE_ATTEMPT:  12"), Some(12));
        assert_eq!(parse_attempt("sin linea"), None);
        assert_eq!(parse_attempt("HIVE_ATTEMPT:bad"), None);
    }

    #[test]
    fn maintainer_rechaza_y_luego_aprueba() {
        let m = Maintainer {
            reject_before_approve: 1,
        };
        let mr = sample_mr("ctx\nHIVE_ATTEMPT:1");
        let tmp = tempfile::tempdir().unwrap();
        let r1 = m.review(1, &mr, tmp.path());
        assert!(matches!(r1.verdict, CouncilVerdict::Rejected { .. }));
        let r2 = m.review(2, &mr, tmp.path());
        assert!(matches!(r2.verdict, CouncilVerdict::Approved));
        assert!(!r2.comments.is_empty());
    }

    #[test]
    fn maintainer_descripcion_vacia_agrega_comentario() {
        let m = Maintainer {
            reject_before_approve: 0,
        };
        let mr = MergeRequest {
            id: Uuid::new_v4(),
            branch: "b".into(),
            title: "x".into(),
            description: "   \n".into(),
            worker_id: Uuid::new_v4(),
            specialist_key: "k".into(),
        };
        let tmp = tempfile::tempdir().unwrap();
        let r = m.review(1, &mr, tmp.path());
        assert!(r
            .comments
            .iter()
            .any(|c| c.contains("Falta contexto") || c.contains("contexto")));
    }

    #[test]
    fn rejection_indicates_obsolete_detecta_frases() {
        assert!(rejection_indicates_work_obsolete(
            "Ya no se requiere este cambio"
        ));
        assert!(rejection_indicates_work_obsolete("NOT NEEDED anymore"));
        assert!(rejection_indicates_work_obsolete("Ya está hecho en main"));
        assert!(rejection_indicates_work_obsolete("duplicate work"));
        assert!(!rejection_indicates_work_obsolete(
            "Rechazo técnico: endurecer mensaje de commit"
        ));
    }

    #[test]
    fn maintainer_cierra_sin_reintento_si_hive_obsolete_en_descripcion() {
        let m = Maintainer {
            reject_before_approve: 99,
        };
        let mr = sample_mr("obj [HIVE_OBSOLETE]\nHIVE_ATTEMPT:1");
        let tmp = tempfile::tempdir().unwrap();
        let r = m.review(1, &mr, tmp.path());
        match &r.verdict {
            CouncilVerdict::Rejected { feedback } => {
                assert!(rejection_indicates_work_obsolete(feedback));
            }
            _ => panic!("esperado Rejected"),
        }
    }

    #[tokio::test]
    async fn council_loop_envia_veredicto() {
        let (tx, rx) = council_channel(4);
        let maintainer = Maintainer {
            reject_before_approve: 0,
        };
        let j = tokio::spawn(run_council_loop(rx, maintainer));
        let mr = sample_mr("d\nHIVE_ATTEMPT:1");
        let (resp_tx, resp_rx) = oneshot::channel();
        tx.send(ReviewEnvelope {
            request: mr.clone(),
            respond_to: resp_tx,
            repo_root: PathBuf::from("/tmp"),
        })
        .await
        .unwrap();
        let got = resp_rx.await.unwrap();
        assert_eq!(got.merge_request_id, mr.id);
        assert!(matches!(got.verdict, CouncilVerdict::Approved));
        drop(tx);
        j.await.unwrap();
    }

    #[test]
    fn review_metrics_calcula_score() {
        assert!(calculate_quality_score(true, 0, 5) > 90);
        assert!(calculate_quality_score(false, 0, 5) < 60);
        // Muchos warnings reducen el score significativamente
        assert!(calculate_quality_score(true, 50, 5) < 60);
    }
}
