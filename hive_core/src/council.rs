use serde::{Deserialize, Serialize};
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
}

#[derive(Debug, Clone)]
pub enum CouncilVerdict {
    Approved,
    Rejected { feedback: String },
}

pub struct ReviewEnvelope {
    pub request: MergeRequest,
    pub respond_to: oneshot::Sender<MaintainerReview>,
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

impl Maintainer {
    pub fn review(&self, attempt: u32, mr: &MergeRequest) -> MaintainerReview {
        if mr.description.contains("[HIVE_OBSOLETE]")
            || mr.description.contains("HIVE_MR_NO_RETRY")
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
            };
        }

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
        comments.push(
            "[Mantenedor] Verificación: cambios aislados en rama no-main; listo para política Hive."
                .into(),
        );

        if attempt <= self.reject_before_approve {
            comments.push(
                "[Mantenedor] Rechazo técnico: endurecer mensaje de commit y añadir nota en hive artefacto."
                    .into(),
            );
            MaintainerReview {
                merge_request_id: mr.id,
                comments: comments.clone(),
                verdict: CouncilVerdict::Rejected {
                    feedback: comments.join("\n"),
                },
            }
        } else {
            comments.push(
                "[Mantenedor] Aprobado: cumple ciclo de revisión y comentarios resueltos.".into(),
            );
            MaintainerReview {
                merge_request_id: mr.id,
                comments: comments.clone(),
                verdict: CouncilVerdict::Approved,
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
        let review = maintainer.review(attempt, &env.request);
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
        let r1 = m.review(1, &mr);
        assert!(matches!(r1.verdict, CouncilVerdict::Rejected { .. }));
        let r2 = m.review(2, &mr);
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
        let r = m.review(1, &mr);
        assert!(
            r.comments
                .iter()
                .any(|c| c.contains("Falta contexto") || c.contains("contexto"))
        );
    }

    #[test]
    fn rejection_indicates_obsolete_detecta_frases() {
        assert!(rejection_indicates_work_obsolete("Ya no se requiere este cambio"));
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
        let r = m.review(1, &mr);
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
        })
        .await
        .unwrap();
        let got = resp_rx.await.unwrap();
        assert_eq!(got.merge_request_id, mr.id);
        assert!(matches!(got.verdict, CouncilVerdict::Approved));
        drop(tx);
        j.await.unwrap();
    }
}
