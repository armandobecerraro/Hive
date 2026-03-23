use crate::agent::WorkerAgent;
use crate::config::HiveConfig;
use crate::council::{
    self, CouncilSender, Maintainer, MaintainerReview, MergeRequest, ReviewEnvelope,
};
use crate::discovery::{RepositoryProfile, SpecialistProfile};
use crate::git_manager::GitManager;
use crate::request::{self, HiveRequest, HiveWorkMode};
use crate::state::HiveState;
use anyhow::{anyhow, Context, Result};
use git2::{BranchType, Oid, Repository, Signature, Time};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

#[cfg(feature = "multiagent")]
use crate::blackboard::Specialist;
#[cfg(feature = "multiagent")]
use crate::brain::{Brain, MockLLMClient};

pub use crate::work::{TaskStatus, WorkerBranchMode, WorkerTask};

/// Main orchestrator for The Hive
pub struct HiveOrchestrator {
    target: std::path::PathBuf,
    profile: crate::discovery::RepositoryProfile,
}

impl HiveOrchestrator {
    pub fn new(target: std::path::PathBuf, profile: crate::discovery::RepositoryProfile) -> Self {
        Self { target, profile }
    }

    pub async fn start(
        &mut self,
        cfg: &HiveConfig,
        hive_request: &HiveRequest,
        work_mode: HiveWorkMode,
    ) -> Result<()> {
        cfg.validate()?;
        let target = self.target.clone();
        let specialists = self.profile.specialists.clone();

        let hive_state =
            std::sync::Arc::new(Mutex::new(crate::state::HiveState::load_or_new(&target)?));
        {
            let mut st = hive_state.lock().await;
            st.trim_retention(cfg.max_decision_records, cfg.max_version_history_entries);
            st.log_decision(
                "colonización_completa",
                format!(
                    "lenguajes={:?}; especialistas={}",
                    self.profile.languages,
                    specialists.len()
                ),
            );
            st.save(&target)?;
        }

        let (council_tx, council_rx) = crate::council::council_channel(64);
        let maintainer = Maintainer {
            reject_before_approve: cfg.maintainer_reject_before_approve,
        };
        tokio::spawn(crate::council::run_council_loop(council_rx, maintainer));

        let branch_mode = match std::env::var("HIVE_BRANCH_MODE").as_deref() {
            Ok("orphan") | Ok("OrphanRoot") => WorkerBranchMode::OrphanRoot,
            _ => WorkerBranchMode::DerivedFromMain,
        };

        let one = request::mission_one_liner(hive_request);
        let brief = request::mission_brief_markdown(hive_request);
        let dest = cfg.integration_branch.as_str();

        let tasks: Vec<WorkerTask> = specialists
            .into_iter()
            .map(|specialist| WorkerTask {
                id: Uuid::new_v4(),
                title: match work_mode {
                    HiveWorkMode::Improve => {
                        if one.is_empty() {
                            format!(
                                "Revisión y mejora [{}] — MR → Consejo → {dest}",
                                specialist.language_key
                            )
                        } else {
                            format!(
                                "Revisión y mejora [{}] · {} — MR → Consejo → {dest}",
                                specialist.language_key, one
                            )
                        }
                    }
                    HiveWorkMode::Build => {
                        if one.is_empty() {
                            format!(
                                "Construcción [{}] — MR → Consejo → {dest}",
                                specialist.language_key
                            )
                        } else {
                            format!(
                                "Construcción [{}] · {} — MR → Consejo → {dest}",
                                specialist.language_key, one
                            )
                        }
                    }
                    HiveWorkMode::Auto => {
                        format!(
                            "Artefacto obrero ({}) — MR → Consejo → {dest}",
                            specialist.language_key
                        )
                    }
                },
                specialist,
                branch_mode,
                mission_one_liner: one.clone(),
                mission_brief: brief.clone(),
                work_mode,
            })
            .collect();

        if tasks.is_empty() {
            return Err(anyhow!("ningún especialista deducido"));
        }

        info!(
            task_count = tasks.len(),
            ?work_mode,
            "iniciando orquestación: una tarea por especialista deducido"
        );
        for t in &tasks {
            info!(specialist = %t.specialist.language_key, title = %t.title, "tarea en cola");
        }

        let orchestrator = Orchestrator {
            repo_root: target.clone(),
            council_tx,
            resource_policy: ResourcePolicy::default(),
            profile: self.profile.clone(),
            max_mr_rejection_attempts: cfg.max_mr_rejection_attempts,
            run_tests_before_mr: cfg.run_tests_before_mr,
            protect_main: cfg.protect_main,
            integration_branch: cfg.integration_branch.clone(),
        };

        run_worker_queue(orchestrator, tasks, hive_state.clone()).await?;

        {
            let mut st = hive_state.lock().await;
            st.trim_retention(cfg.max_decision_records, cfg.max_version_history_entries);
            st.save(&target)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionFile {
    pub version: String,
}

#[derive(Clone)]
pub struct ResourcePolicy {
    pub max_cpu_percent: f32,
    pub min_available_ram_bytes: u64,
}

impl Default for ResourcePolicy {
    fn default() -> Self {
        Self {
            max_cpu_percent: 88.0,
            min_available_ram_bytes: 256 * 1024 * 1024,
        }
    }
}

pub fn system_allows_new_instance(policy: &ResourcePolicy) -> bool {
    use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

    let mut sys = System::new_with_specifics(
        RefreshKind::new()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::everything()),
    );
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let cpus = sys.cpus();
    let mut avg = if cpus.is_empty() {
        0.0
    } else {
        cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32
    };
    if avg.is_nan() || avg.is_infinite() {
        avg = 0.0;
    }
    let avail = sys.available_memory();
    let ram_ok = policy.min_available_ram_bytes == 0 || avail > policy.min_available_ram_bytes;
    avg < policy.max_cpu_percent && ram_ok
}

/// En tests o con `HIVE_SKIP_RESOURCE_GATE=1` no se bloquea el arranque de obreras por CPU/RAM.
fn skip_resource_wait_for_tests() -> bool {
    cfg!(test) || std::env::var("HIVE_SKIP_RESOURCE_GATE").as_deref() == Ok("1")
}

pub struct Orchestrator {
    pub repo_root: PathBuf,
    pub council_tx: CouncilSender,
    pub resource_policy: ResourcePolicy,
    pub profile: RepositoryProfile,
    pub max_mr_rejection_attempts: u32,
    pub run_tests_before_mr: bool,
    /// Si es true, merges y hitos van a `integration_branch`, no a `main`.
    pub protect_main: bool,
    /// Rama destino de merges aprobados y documentación de hitos.
    pub integration_branch: String,
}

impl Orchestrator {
    /// Incuba una **instancia** obrera (tarea Tokio aislada): espera recursos si hace falta, luego ejecuta ciclo MR ↔ Consejo.
    pub async fn incubate_worker_instance(
        self,
        task: WorkerTask,
        state: ArcMutexState,
    ) -> Result<()> {
        let repo_root = self.repo_root.clone();
        let council_tx = self.council_tx.clone();
        let policy = self.resource_policy;

        loop {
            if skip_resource_wait_for_tests() || system_allows_new_instance(&policy) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }

        let profile = self.profile.clone();
        let max_mr = self.max_mr_rejection_attempts;
        let run_tests = self.run_tests_before_mr;
        let protect_main = self.protect_main;
        let integration_branch = self.integration_branch.clone();
        let handle = tokio::spawn(async move {
            run_worker_lifecycle(
                repo_root,
                council_tx,
                task,
                state,
                profile,
                max_mr,
                run_tests,
                protect_main,
                integration_branch,
            )
            .await
        });
        handle.await??;
        Ok(())
    }
}

type ArcMutexState = std::sync::Arc<Mutex<HiveState>>;

fn hive_signature(name: &'static str, email: &'static str) -> Result<Signature<'static>> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let time = Time::new(ts, 0);
    Signature::new(name, email, &time).context("firma git")
}

/// Ciclo tipo GitLab/GitHub: rama por obrera → commits en esa rama → MR (revisión Consejo) → merge a la línea de integración → borrar rama local.
#[allow(clippy::too_many_arguments)] // parámetros de política Git + ciclo MR explícitos
async fn run_worker_lifecycle(
    repo_root: PathBuf,
    council_tx: CouncilSender,
    task: WorkerTask,
    state: ArcMutexState,
    profile: RepositoryProfile,
    max_mr_rejection_attempts: u32,
    run_tests_before_mr: bool,
    protect_main: bool,
    integration_branch: String,
) -> Result<()> {
    let branch = format!("hive/worker/{}", task.id);
    let mut attempt: u32 = 1;
    // Tras un rechazo, el siguiente intento entrega el texto del Consejo a la obrera (ver `.hive/last_council_feedback.md`).
    let mut pending_council_feedback: Option<String> = None;

    info!(%branch, "rama obrera");

    loop {
        let evolution_stamp = format!(
            "{} · MR intento {}",
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
            attempt
        );

        let orphan_first = task.branch_mode == WorkerBranchMode::OrphanRoot && attempt == 1;
        let rr = repo_root.clone();
        let b = branch.clone();
        let legacy_orphan = if orphan_first {
            tokio::task::spawn_blocking(move || {
                let repo = Repository::open(&rr)?;
                Ok::<_, anyhow::Error>(!branch_exists(&repo, &b)?)
            })
            .await
            .context("join branch_exists")??
        } else {
            false
        };

        if legacy_orphan {
            let rr = repo_root.clone();
            let b = branch.clone();
            let spec = task.specialist.clone();
            let title = task.title.clone();
            let tid = task.id;
            let mode = task.branch_mode;
            let pm = protect_main;
            let ib = integration_branch.clone();
            info!(attempt, "commit artefacto obrero (rama huérfana inicial)");
            tokio::task::spawn_blocking(move || {
                worker_commit_on_branch(&rr, &b, &spec, &title, tid, attempt, mode, pm, &ib)
            })
            .await
            .context("join worker_commit")??;
        } else {
            let rr = repo_root.clone();
            let b = branch.clone();
            let mode = task.branch_mode;
            let att = attempt;
            let pm = protect_main;
            let ib = integration_branch.clone();
            tokio::task::spawn_blocking(move || {
                ensure_on_worker_branch(&rr, &b, mode, att, pm, &ib)
            })
            .await
            .context("join ensure branch")??;

            let mut agent = WorkerAgent::with_evolution(
                task.clone(),
                repo_root.clone(),
                profile.clone(),
                GitManager::new(),
                evolution_stamp,
            );

            #[cfg(feature = "multiagent")]
            {
                // Crear Brain con cliente LLM (usar mock si no hay Ollama)
                let specialist = match task.specialist.language_key.as_str() {
                    "rust" => Specialist::Rust,
                    "python" => Specialist::Python,
                    "javascript" => Specialist::JavaScript,
                    "typescript" => Specialist::TypeScript,
                    "test" | "test_engineer" => Specialist::Test,
                    "docs" => Specialist::Docs,
                    _ => Specialist::Generic,
                };

                let llm_client: Box<dyn crate::brain::LLMClient> = if std::env::var(
                    "HIVE_USE_MOCK_LLM",
                )
                .unwrap_or_default()
                    == "1"
                {
                    Box::new(MockLLMClient::new(
                            "[FILE: src/generated.rs]\n[ACTION: create]\n[CODE]\n// Generated by The Hive\nfn placeholder() {}\n[/CODE]".to_string()
                        ))
                } else if let Ok(ollama_url) = std::env::var("OLLAMA_BASE_URL") {
                    let model =
                        std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "codellama".to_string());
                    Box::new(crate::brain::OllamaClient::new(&ollama_url, &model))
                } else {
                    // Fallback a mock si no hay LLM configurado
                    Box::new(MockLLMClient::new(
                            format!(
                                "[FILE: src/generated_{}.rs]\n[ACTION: create]\n[CODE]\n// Generated by The Hive for {} task\n// Task: {}\nfn generated_function() {{}}\n[/CODE]",
                                specialist.to_string().to_lowercase(),
                                task.specialist.language_key,
                                task.title.replace('\n', " ")
                            )
                        ))
                };

                let blackboard = std::sync::Arc::new(crate::blackboard::Blackboard::new());
                let brain = std::sync::Arc::new(Brain::new(llm_client, blackboard, specialist));
                agent = agent.with_brain(brain);

                info!(
                    specialist = %task.specialist.language_key,
                    "WorkerAgent configurado con Brain LLM"
                );
            }

            if let Some(fb) = pending_council_feedback.take() {
                agent.mark_as_needs_revision(vec![fb]);
            }
            agent
                .execute_task()
                .await
                .map_err(|e| anyhow!("obrera simulada: {e}"))?;

            let rr = repo_root.clone();
            let b = branch.clone();
            let spec = task.specialist.clone();
            let title = task.title.clone();
            let tid = task.id;
            let mode = task.branch_mode;
            info!(attempt, "commit cambios de código y artefacto obrero");
            tokio::task::spawn_blocking(move || {
                worker_stage_workspace_commit(&rr, &b, &spec, &title, tid, attempt, mode)
            })
            .await
            .context("join stage commit")??;
        }

        if run_tests_before_mr {
            let rr = repo_root.clone();
            tokio::task::spawn_blocking(move || preflight_cargo_test(&rr))
                .await
                .context("join preflight cargo test")??;
        }

        let mr = MergeRequest {
            id: Uuid::new_v4(),
            branch: branch.clone(),
            title: task.title.clone(),
            description: format!(
                "{}\n\n---\nAutomatizado por The Hive.\nHIVE_ATTEMPT:{attempt}\nEspecialista: {}\n{}",
                task.mission_brief,
                task.specialist.language_key,
                task.specialist.prompt_blueprint
            ),
            worker_id: task.id,
            specialist_key: task.specialist.language_key.clone(),
        };

        info!(mr_id = %mr.id, "MR creado; envío al Consejo");
        let (tx, rx) = tokio::sync::oneshot::channel();
        let repo_root_for_council = repo_root.clone();
        council_tx
            .send(ReviewEnvelope {
                request: mr.clone(),
                respond_to: tx,
                repo_root: repo_root_for_council,
            })
            .await
            .map_err(|_| anyhow!("canal del Consejo cerrado"))?;

        let review: MaintainerReview =
            rx.await.map_err(|_| anyhow!("sin respuesta del Consejo"))?;

        match review.verdict {
            council::CouncilVerdict::Approved => {
                info!(mr_id = %mr.id, "Consejo aprobó MR");
                let repo_root2 = repo_root.clone();
                let branch2 = branch.clone();
                let merge_mode = task.branch_mode;
                let mr_id = mr.id;
                let specialist_key = task.specialist.language_key.clone();
                let target = integration_branch.clone();
                tokio::task::spawn_blocking(move || {
                    merge_branch_into_target(&repo_root2, &branch2, merge_mode, &target)?;
                    bump_version_json(&repo_root2, &target)?;
                    let ver = read_version_from_disk(&repo_root2)?;
                    commit_hive_objective_milestone(
                        &repo_root2,
                        &ver,
                        &branch2,
                        mr_id,
                        &specialist_key,
                        &target,
                    )?;
                    Ok::<_, anyhow::Error>(mr_id)
                })
                .await
                .context("join merge")??;

                let ver = read_version_string(&repo_root).await?;
                {
                    let mut st = state.lock().await;
                    st.log_decision(
                        "merge_aprobado",
                        format!("rama {} fusionada; MR {}", branch, mr.id),
                    );
                    st.record_merge(ver, mr.id, branch.clone());
                    st.save(&repo_root)?;
                }
                return Ok(());
            }
            council::CouncilVerdict::Rejected { feedback } => {
                warn!(mr_id = %mr.id, %feedback, "Consejo rechazó MR");

                if council::rejection_indicates_work_obsolete(&feedback) {
                    warn!(
                        %branch,
                        worker = %task.id,
                        "rechazo por trabajo obsoleto o innecesario: sin más reintentos"
                    );
                    let repo_obs = repo_root.clone();
                    let fb_obs = feedback.clone();
                    let att = attempt;
                    let br_obs = branch.clone();
                    let wid_obs = task.id;
                    let sk_obs = task.specialist.language_key.clone();
                    let line_obs = integration_branch.clone();
                    tokio::task::spawn_blocking(move || {
                        append_feedback_note(&repo_obs, &fb_obs, att)?;
                        abandon_worker_mr(
                            &repo_obs,
                            &br_obs,
                            wid_obs,
                            &sk_obs,
                            "trabajo obsoleto o ya no requerido (sin reintentos)",
                            &line_obs,
                        )
                    })
                    .await
                    .context("join abandon obsoleto")??;
                    {
                        let mut st = state.lock().await;
                        st.log_decision(
                            "mr_descartado_obsoleto",
                            format!(
                                "rama {branch}; worker {}; especialista {}; sin reintento por feedback del Mantenedor",
                                task.id, task.specialist.language_key
                            ),
                        );
                        st.save(&repo_root)?;
                    }
                    return Ok(());
                }

                pending_council_feedback = Some(feedback.clone());
                let repo_root3 = repo_root.clone();
                let fb = feedback.clone();
                tokio::task::spawn_blocking(move || {
                    append_feedback_note(&repo_root3, &fb, attempt)
                })
                .await
                .context("join feedback note")??;

                {
                    let mut st = state.lock().await;
                    st.log_decision(
                        "mr_rechazado",
                        format!("intento {attempt}; comentarios registrados en rama; obrera reintentará con feedback"),
                    );
                    st.save(&repo_root)?;
                }

                attempt += 1;
                if attempt > max_mr_rejection_attempts {
                    warn!(
                        %branch,
                        worker = %task.id,
                        specialist = %task.specialist.language_key,
                        limit = max_mr_rejection_attempts,
                        "Consejo rechazó demasiadas veces: se descarta la rama obrera y la cola sigue"
                    );
                    let rr = repo_root.clone();
                    let br = branch.clone();
                    let wid = task.id;
                    let sk = task.specialist.language_key.clone();
                    let line_ab = integration_branch.clone();
                    tokio::task::spawn_blocking(move || {
                        abandon_worker_mr(
                            &rr,
                            &br,
                            wid,
                            &sk,
                            "máximo de rechazos del Consejo alcanzado",
                            &line_ab,
                        )
                    })
                    .await
                    .context("join abandon obrera")??;
                    {
                        let mut st = state.lock().await;
                        st.log_decision(
                            "mr_descartado_max_rechazos",
                            format!(
                                "rama {branch}; worker {}; especialista {}; límite {max_mr_rejection_attempts}",
                                task.id, task.specialist.language_key
                            ),
                        );
                        st.save(&repo_root)?;
                    }
                    return Ok(());
                }
            }
        }
    }
}

fn append_feedback_note(repo_root: &Path, feedback: &str, attempt: u32) -> Result<()> {
    let repo = Repository::open(repo_root)?;
    let p = repo_root.join("hive_council_feedback.log");
    let line = format!(
        "\n--- attempt {attempt} ---\n{feedback}\n",
        attempt = attempt,
        feedback = feedback
    );
    let mut prev = String::new();
    if p.exists() {
        prev = std::fs::read_to_string(&p).unwrap_or_default();
    }
    std::fs::write(&p, format!("{prev}{line}"))?;

    let sig = hive_signature("The Hive", "council@hive.local")?;
    let parent = repo.head()?.peel_to_commit()?;
    let mut index = repo.index()?;
    // Sincronizar el índice con el árbol de HEAD: un `.git/index` sucio (p. ej. tras operaciones
    // previas en la rama obrera) puede provocar `write_tree` → "invalid object specified".
    let head_tree = parent.tree()?;
    index.read_tree(&head_tree)?;
    index.add_path(
        p.strip_prefix(repo_root)
            .map_err(|_| anyhow!("ruta feedback fuera del repo"))?,
    )?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &format!("chore(hive): council feedback attempt {attempt}"),
        &tree,
        &[&parent],
    )?;
    Ok(())
}

fn read_version_from_disk(repo_root: &Path) -> Result<String> {
    let p = repo_root.join("version.json");
    let raw = std::fs::read_to_string(&p).unwrap_or_else(|_| r#"{"version":"0.0.0"}"#.into());
    let v: VersionFile = serde_json::from_str(&raw).unwrap_or(VersionFile {
        version: "0.0.0".into(),
    });
    Ok(v.version)
}

async fn read_version_string(repo_root: &Path) -> Result<String> {
    let root = repo_root.to_path_buf();
    tokio::task::spawn_blocking(move || read_version_from_disk(&root))
        .await
        .context("join read_version")?
}

/// Tras cada merge a la línea de integración, deja una línea verificable del **objetivo** (integración revisada + versión).
fn commit_hive_objective_milestone(
    repo_root: &Path,
    version: &str,
    merged_branch: &str,
    mr_id: Uuid,
    specialist_key: &str,
    integration_branch: &str,
) -> Result<()> {
    let p = repo_root.join("HIVE_OBJECTIVE.md");
    let header = format!(
        "# Objetivo operativo (Hive)\n\n\
        **Meta:** integrar en `{ib}` cambios generados por cada especialista, pasando revisión del Consejo, \
        y subir la versión en `version.json`.\n\n\
        ## Hitos en `{ib}`\n\n",
        ib = integration_branch
    );
    let mut body = if p.exists() {
        std::fs::read_to_string(&p)?
    } else {
        header
    };
    let dedupe_key = format!("rama `{merged_branch}` · MR `{mr_id}`");
    if body.contains(&dedupe_key) {
        return Ok(());
    }
    let stamp = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC");
    let line = format!(
        "- [x] {stamp} · `v{version}` · especialista `{specialist_key}` · rama `{merged_branch}` · MR `{mr_id}`\n"
    );
    body.push_str(&line);
    std::fs::write(&p, &body)?;

    let repo = Repository::open(repo_root)?;
    checkout_branch(&repo, integration_branch)?;
    let sig = hive_signature("The Hive", "objective@hive.local")?;
    let head = repo.head()?.peel_to_commit()?;
    let mut index = repo.index()?;
    index.add_path(
        p.strip_prefix(repo_root)
            .map_err(|_| anyhow!("HIVE_OBJECTIVE.md fuera del repo"))?,
    )?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &format!("docs(hive): hito objetivo v{version}"),
        &tree,
        &[&head],
    )?;
    Ok(())
}

fn worker_artifact_body(
    worker_id: Uuid,
    attempt: u32,
    specialist: &SpecialistProfile,
    title: &str,
    branch_mode: WorkerBranchMode,
) -> String {
    format!(
        "# Hive worker {worker_id}\n\nIntento {attempt} · modo rama: {mode:?}\n\n## Especialista deducido\n- `{}`\n\n## Herramientas sugeridas\n{}\n\n## Tarea\n{title}\n",
        specialist.language_key,
        specialist
            .suggested_tools
            .iter()
            .map(|t| format!("- `{t}`"))
            .collect::<Vec<_>>()
            .join("\n"),
        worker_id = worker_id,
        attempt = attempt,
        title = title,
        mode = branch_mode,
    )
}

fn ensure_on_worker_branch(
    repo_root: &Path,
    branch: &str,
    branch_mode: WorkerBranchMode,
    attempt: u32,
    protect_main: bool,
    integration_branch: &str,
) -> Result<()> {
    ensure_mainline_seed(repo_root)?;
    ensure_integration_branch(repo_root, integration_branch, protect_main)?;
    let repo = Repository::open(repo_root)?;
    if branch_mode == WorkerBranchMode::OrphanRoot && attempt == 1 && !branch_exists(&repo, branch)?
    {
        return Err(anyhow!(
            "ensure_on_worker_branch: huérfano inicial debe usar worker_commit_on_branch"
        ));
    }
    if protect_main {
        checkout_branch(&repo, integration_branch)?;
    }
    let head = repo.head()?.peel_to_commit()?;
    if branch_exists(&repo, branch)? {
        checkout_branch(&repo, branch)?;
    } else {
        repo.branch(branch, &head, false)?;
        checkout_branch(&repo, branch)?;
    }
    crate::purge_dot_hive_worker_artifacts(repo_root);
    Ok(())
}

fn worker_stage_workspace_commit(
    repo_root: &Path,
    branch: &str,
    specialist: &SpecialistProfile,
    title: &str,
    worker_id: Uuid,
    attempt: u32,
    branch_mode: WorkerBranchMode,
) -> Result<()> {
    let n = crate::purge_dot_hive_worker_artifacts(repo_root);
    if n > 0 {
        tracing::debug!(count = n, "purga .hive_worker_*.md antes de stage obrero");
    }

    let hive_meta = repo_root.join(".hive");
    std::fs::create_dir_all(&hive_meta)?;
    let artifact = hive_meta.join("worker_artifact.md");
    let body = worker_artifact_body(worker_id, attempt, specialist, title, branch_mode);
    std::fs::write(&artifact, &body)?;

    let repo = Repository::open(repo_root)?;
    let head_name = head_branch_name(&repo)?;
    if head_name != branch {
        anyhow::bail!("HEAD es {head_name}, se esperaba rama obrera {branch}");
    }

    let sig = hive_signature("Hive Worker", "worker@hive.local")?;
    let mut index = repo.index()?;
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
    index.write()?;
    let oid = index.write_tree()?;
    let tree = repo.find_tree(oid)?;
    let parent = repo.head()?.peel_to_commit()?;
    repo.commit(
        Some(&format!("refs/heads/{branch}")),
        &sig,
        &sig,
        &format!("feat(hive): obrera {worker_id} intento {attempt} (código + artefacto)"),
        &tree,
        &[&parent],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn worker_commit_on_branch(
    repo_root: &Path,
    branch: &str,
    specialist: &SpecialistProfile,
    title: &str,
    worker_id: Uuid,
    attempt: u32,
    branch_mode: WorkerBranchMode,
    protect_main: bool,
    integration_branch: &str,
) -> Result<()> {
    ensure_mainline_seed(repo_root)?;
    ensure_integration_branch(repo_root, integration_branch, protect_main)?;
    let repo = Repository::open(repo_root)?;

    let n = crate::purge_dot_hive_worker_artifacts(repo_root);
    if n > 0 {
        tracing::debug!(count = n, "purga .hive_worker_*.md (commit obrero legado)");
    }

    // Un solo archivo reutilizable por ciclo: evita llenar la raíz de `.hive_worker_<uuid>.md`
    // (eran solo plantillas para simular un entregable con contenido en Git).
    let hive_meta = repo_root.join(".hive");
    std::fs::create_dir_all(&hive_meta)?;
    let artifact = hive_meta.join("worker_artifact.md");
    let body = worker_artifact_body(worker_id, attempt, specialist, title, branch_mode);
    std::fs::write(&artifact, &body)?;

    if branch_mode == WorkerBranchMode::OrphanRoot && attempt == 1 && !branch_exists(&repo, branch)?
    {
        let base = orphan_initial_base_branch(&repo, protect_main, integration_branch)?;
        checkout_branch(&repo, &base)?;
        let mut index = repo.index()?;
        index.clear()?;
        index.add_path(
            artifact
                .strip_prefix(repo_root)
                .map_err(|_| anyhow!("artefacto fuera del repo"))?,
        )?;
        let oid = index.write_tree()?;
        let tree = repo.find_tree(oid)?;
        let sig = hive_signature("Hive Worker", "worker@hive.local")?;
        repo.commit(
            Some(&format!("refs/heads/{branch}")),
            &sig,
            &sig,
            &format!("feat(hive): obrera {worker_id} intento {attempt} (rama huérfana)"),
            &tree,
            &[],
        )?;
        checkout_branch(&repo, branch)?;
        crate::purge_dot_hive_worker_artifacts(repo_root);
        return Ok(());
    }

    if protect_main {
        checkout_branch(&repo, integration_branch)?;
    }

    let head = repo.head()?.peel_to_commit()?;
    if branch_exists(&repo, branch)? {
        checkout_branch(&repo, branch)?;
    } else {
        repo.branch(branch, &head, false)?;
        checkout_branch(&repo, branch)?;
    }

    crate::purge_dot_hive_worker_artifacts(repo_root);

    let sig = hive_signature("Hive Worker", "worker@hive.local")?;
    let mut index = repo.index()?;
    index.add_path(
        artifact
            .strip_prefix(repo_root)
            .map_err(|_| anyhow!("artefacto fuera del repo"))?,
    )?;
    let oid = index.write_tree()?;
    let tree = repo.find_tree(oid)?;
    let parent = repo.head()?.peel_to_commit()?;
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &format!("feat(hive): obrera {worker_id} intento {attempt}"),
        &tree,
        &[&parent],
    )?;
    Ok(())
}

fn head_branch_name(repo: &Repository) -> Result<String> {
    let h = repo.head()?;
    let shorthand = h.shorthand().ok_or_else(|| anyhow!("HEAD sin shorthand"))?;
    Ok(shorthand.to_string())
}

fn branch_exists(repo: &Repository, name: &str) -> Result<bool> {
    Ok(repo.find_branch(name, BranchType::Local).is_ok())
}

fn checkout_branch(repo: &Repository, name: &str) -> Result<()> {
    let refname = format!("refs/heads/{name}");
    repo.set_head(&refname)?;
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::default()
            .force()
            .remove_ignored(false),
    ))?;
    Ok(())
}

/// `main` o `master`, según exista (bootstrap clásico).
fn default_mainline_branch_name(repo: &Repository) -> Result<&'static str> {
    if branch_exists(repo, "main")? {
        Ok("main")
    } else if branch_exists(repo, "master")? {
        Ok("master")
    } else {
        Err(anyhow!(
            "no hay rama main ni master (necesaria para línea de trabajo)"
        ))
    }
}

/// Con `protect_main`, crea `integration_branch` apuntando al mismo commit que `main`/`master` sin modificar esas ramas.
fn ensure_integration_branch(
    repo_root: &Path,
    integration_branch: &str,
    protect_main: bool,
) -> Result<()> {
    if !protect_main {
        return Ok(());
    }
    let repo = Repository::open(repo_root)?;
    if branch_exists(&repo, integration_branch)? {
        return Ok(());
    }
    let base_commit = if branch_exists(&repo, "main")? {
        repo.find_reference("refs/heads/main")
            .context("refs/heads/main")?
            .peel_to_commit()?
    } else if branch_exists(&repo, "master")? {
        repo.find_reference("refs/heads/master")
            .context("refs/heads/master")?
            .peel_to_commit()?
    } else {
        return Err(anyhow!(
            "HIVE_PROTECT_MAIN: se requiere rama `main` o `master` para crear `{integration_branch}`"
        ));
    };
    repo.branch(integration_branch, &base_commit, false)?;
    Ok(())
}

fn orphan_initial_base_branch(
    repo: &Repository,
    protect_main: bool,
    integration_branch: &str,
) -> Result<String> {
    if protect_main {
        if !branch_exists(repo, integration_branch)? {
            anyhow::bail!("falta rama de integración `{integration_branch}`");
        }
        Ok(integration_branch.to_string())
    } else {
        Ok(default_mainline_branch_name(repo)?.to_string())
    }
}

fn checkout_main_or_master(repo: &Repository) -> Result<()> {
    if branch_exists(repo, "main")? {
        checkout_branch(repo, "main")
    } else if branch_exists(repo, "master")? {
        checkout_branch(repo, "master")
    } else {
        Err(anyhow!(
            "no hay rama main ni master para volver tras descartar obrera"
        ))
    }
}

/// Tras demasiados rechazos: vuelve a la línea de integración, borra la rama obrera y deja constancia (sin fusionar).
fn abandon_worker_mr(
    repo_root: &Path,
    worker_branch: &str,
    worker_id: Uuid,
    specialist_key: &str,
    reason: &str,
    integration_line: &str,
) -> Result<()> {
    let repo = Repository::open(repo_root)?;
    if branch_exists(&repo, integration_line)? {
        checkout_branch(&repo, integration_line)?;
    } else {
        checkout_main_or_master(&repo)?;
    }
    let _ = crate::git_manager::delete_local_feature_branch_after_integrated_merge(
        repo_root,
        worker_branch,
    );
    let hive = repo_root.join(".hive");
    std::fs::create_dir_all(&hive).with_context(|| format!("crear {}", hive.display()))?;
    let log = hive.join("abandoned_mrs.log");
    let stamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
    let block = format!(
        "\n--- {stamp} ---\nrama `{worker_branch}` worker={worker_id} specialist={specialist_key}\n{reason}\n"
    );
    let mut prev = std::fs::read_to_string(&log).unwrap_or_default();
    prev.push_str(&block);
    std::fs::write(&log, prev)?;
    Ok(())
}

/// Si hay `Cargo.toml`, exige `cargo test` verde antes de abrir MR al Consejo.
fn preflight_cargo_test(repo_root: &Path) -> Result<()> {
    if !repo_root.join("Cargo.toml").exists() {
        return Ok(());
    }
    let out = std::process::Command::new("cargo")
        .current_dir(repo_root)
        .args(["test", "--no-fail-fast", "-q"])
        .output()
        .context("ejecutar cargo test (¿cargo en PATH?)")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        anyhow::bail!("cargo test falló antes del MR:\n{stdout}\n{stderr}");
    }
    Ok(())
}

fn ensure_mainline_seed(repo_root: &Path) -> Result<()> {
    let repo = Repository::open(repo_root)?;
    if let Ok(head) = repo.head() {
        if head.target().is_some() {
            return Ok(());
        }
    }
    let sig = hive_signature("The Queen", "queen@hive.local")?;
    let mut index = repo.index()?;
    let readme = repo_root.join("README.hive.md");
    if !readme.exists() {
        std::fs::write(
            &readme,
            "# Colonizado por The Hive\n\nRepositorio inicializado por el núcleo Queen.\n",
        )?;
    }
    index.add_path(
        readme
            .strip_prefix(repo_root)
            .map_err(|_| anyhow!("README fuera del repo"))?,
    )?;
    let vf = repo_root.join("version.json");
    if !vf.exists() {
        std::fs::write(&vf, "{\n  \"version\": \"0.1.0\"\n}\n")?;
    }
    index.add_path(
        vf.strip_prefix(repo_root)
            .map_err(|_| anyhow!("version.json fuera del repo"))?,
    )?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let oid = repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        "chore: seed main (Queen)",
        &tree,
        &[],
    )?;
    let cobj = repo.find_commit(oid)?;
    // libgit2 reciente: no forzar `branch(main, …, true)` si HEAD ya es `main` (falla).
    if repo.find_branch("main", BranchType::Local).is_err() {
        repo.branch("main", &cobj, false)?;
    }
    repo.set_head("refs/heads/main")?;
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::default()
            .force()
            .remove_ignored(false),
    ))?;
    Ok(())
}

fn merge_branch_into_target(
    repo_root: &Path,
    worker_branch: &str,
    branch_mode: WorkerBranchMode,
    target_branch: &str,
) -> Result<()> {
    match branch_mode {
        WorkerBranchMode::DerivedFromMain => {
            merge_derived_worker_into_target(repo_root, worker_branch, target_branch)
        }
        WorkerBranchMode::OrphanRoot => {
            merge_orphan_worker_into_target(repo_root, worker_branch, target_branch)
        }
    }
}

fn merge_derived_worker_into_target(
    repo_root: &Path,
    worker_branch: &str,
    target_branch: &str,
) -> Result<()> {
    let repo = Repository::open(repo_root)?;
    let sig = hive_signature("The Queen", "queen@hive.local")?;

    let ref_target = format!("refs/heads/{target_branch}");
    let main_commit = repo
        .find_reference(&ref_target)
        .with_context(|| ref_target.clone())?
        .peel_to_commit()?;
    let worker_commit = repo
        .find_branch(worker_branch, BranchType::Local)
        .with_context(|| format!("rama obrera {worker_branch}"))?
        .get()
        .peel_to_commit()?;

    let mut idx = repo.merge_commits(&main_commit, &worker_commit, None)?;
    if idx.has_conflicts() {
        return Err(anyhow!("conflictos de merge; intervención manual"));
    }
    let tree_id = idx.write_tree_to(&repo)?;
    let tree = repo.find_tree(tree_id)?;
    checkout_branch(&repo, target_branch)?;
    let merge_commit = repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &format!("merge: integrate {worker_branch}"),
        &tree,
        &[&main_commit, &worker_commit],
    )?;

    repo.set_head(&ref_target)?;
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::default()
            .force()
            .remove_ignored(false),
    ))?;

    crate::purge_dot_hive_worker_artifacts(repo_root);

    crate::git_manager::delete_local_feature_branch_after_integrated_merge(
        repo_root,
        worker_branch,
    )?;

    let _ = merge_commit;
    Ok(())
}

/// Fusión de historiales sin ancestro común (equivalente a `--allow-unrelated-histories`): ancestro = árbol vacío estándar.
fn merge_orphan_worker_into_target(
    repo_root: &Path,
    worker_branch: &str,
    target_branch: &str,
) -> Result<()> {
    let repo = Repository::open(repo_root)?;
    let sig = hive_signature("The Queen", "queen@hive.local")?;

    let ref_target = format!("refs/heads/{target_branch}");
    let main_commit = repo
        .find_reference(&ref_target)
        .with_context(|| ref_target.clone())?
        .peel_to_commit()?;
    let worker_commit = repo
        .find_branch(worker_branch, BranchType::Local)
        .with_context(|| format!("rama obrera {worker_branch}"))?
        .get()
        .peel_to_commit()?;

    let empty_oid = Oid::from_str("4b825dc642cb6eb9a060e54bf8d69288fbee4904")
        .map_err(|e| anyhow!("oid árbol vacío: {e}"))?;
    let ancestor_tree = repo.find_tree(empty_oid)?;
    let ours_tree = main_commit.tree()?;
    let theirs_tree = worker_commit.tree()?;

    let mut idx = repo.merge_trees(&ancestor_tree, &ours_tree, &theirs_tree, None)?;
    if idx.has_conflicts() {
        return Err(anyhow!(
            "conflictos en merge huérfano ↔ {target_branch}; intervención manual"
        ));
    }
    let tree_id = idx.write_tree_to(&repo)?;
    let tree = repo.find_tree(tree_id)?;
    checkout_branch(&repo, target_branch)?;
    let merge_commit = repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &format!("merge: integrate orphan {worker_branch}"),
        &tree,
        &[&main_commit, &worker_commit],
    )?;

    repo.set_head(&ref_target)?;
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::default()
            .force()
            .remove_ignored(false),
    ))?;

    crate::purge_dot_hive_worker_artifacts(repo_root);

    crate::git_manager::delete_local_feature_branch_after_integrated_merge(
        repo_root,
        worker_branch,
    )?;

    let _ = merge_commit;
    Ok(())
}

fn bump_version_json(repo_root: &Path, target_branch: &str) -> Result<()> {
    let p = repo_root.join("version.json");
    let raw = std::fs::read_to_string(&p).unwrap_or_else(|_| r#"{"version":"0.0.0"}"#.into());
    let mut vf: VersionFile = serde_json::from_str(&raw).unwrap_or(VersionFile {
        version: "0.0.0".into(),
    });
    vf.version = bump_patch(&vf.version);
    std::fs::write(&p, serde_json::to_string_pretty(&vf)?)?;

    let repo = Repository::open(repo_root)?;
    checkout_branch(&repo, target_branch)?;
    let sig = hive_signature("The Queen", "queen@hive.local")?;
    let head = repo.head()?.peel_to_commit()?;
    let mut index = repo.index()?;
    index.add_path(
        p.strip_prefix(repo_root)
            .map_err(|_| anyhow!("version.json fuera del repo"))?,
    )?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &format!("chore(release): {}", vf.version),
        &tree,
        &[&head],
    )?;
    Ok(())
}

fn bump_patch(v: &str) -> String {
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() != 3 {
        return format!("{v}.1");
    }
    let major = parts[0];
    let minor = parts[1];
    let patch: u32 = parts[2].parse::<u32>().unwrap_or(0).saturating_add(1);
    format!("{major}.{minor}.{patch}")
}

/// Cola de trabajo: las tareas se **encolan** en orden; si la CPU/RAM no dan, se espera antes de incubar la siguiente instancia.
/// Sin tareas, el orquestador no crea obreras (autodestrucción lógica: ningún `JoinHandle` vivo).
pub async fn run_worker_queue(
    orchestrator: Orchestrator,
    tasks: Vec<WorkerTask>,
    state: ArcMutexState,
) -> Result<()> {
    if tasks.is_empty() {
        warn!("cola vacía: ninguna instancia obrera incubada");
        return Ok(());
    }
    let mut pending = VecDeque::from(tasks);
    while let Some(t) = pending.pop_front() {
        while !skip_resource_wait_for_tests()
            && !system_allows_new_instance(&orchestrator.resource_policy)
        {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        let spec = t.specialist.language_key.clone();
        let orch = Orchestrator {
            repo_root: orchestrator.repo_root.clone(),
            council_tx: orchestrator.council_tx.clone(),
            resource_policy: orchestrator.resource_policy.clone(),
            profile: orchestrator.profile.clone(),
            max_mr_rejection_attempts: orchestrator.max_mr_rejection_attempts,
            run_tests_before_mr: orchestrator.run_tests_before_mr,
            protect_main: orchestrator.protect_main,
            integration_branch: orchestrator.integration_branch.clone(),
        };
        if let Err(e) = orch.incubate_worker_instance(t, state.clone()).await {
            warn!(
                error = %e,
                specialist = %spec,
                "obrera falló o abortó; el resto de la cola continúa"
            );
        }
    }
    Ok(())
}

/// Compatibilidad: alias de [`run_worker_queue`].
pub async fn drain_task_queue(
    orchestrator: Orchestrator,
    tasks: Vec<WorkerTask>,
    state: ArcMutexState,
) -> Result<()> {
    run_worker_queue(orchestrator, tasks, state).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HiveConfig;
    use crate::council::{self, Maintainer};
    use crate::discovery::{
        colonize_and_analyze, deduce_specialists, RepoDna, RepositoryAnalyzer, RepositoryProfile,
    };

    fn empty_profile() -> RepositoryProfile {
        RepositoryProfile {
            languages: vec![],
            frameworks: vec![],
            technical_debt: Default::default(),
            specialists: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        }
    }
    use crate::state::HiveState;
    use git2::Repository;
    use serial_test::serial;
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;
    use tokio::sync::Mutex;

    #[test]
    fn bump_patch_semver() {
        assert_eq!(bump_patch("1.2.3"), "1.2.4");
    }

    #[test]
    fn bump_patch_no_semver_agrega_sufijo() {
        assert_eq!(bump_patch("v1"), "v1.1");
    }

    #[test]
    fn hive_objective_milestone_creates_and_commits() {
        let tmp = tempfile::tempdir().unwrap();
        colonize_and_analyze(tmp.path()).unwrap();
        ensure_mainline_seed(tmp.path()).unwrap();
        let id = Uuid::new_v4();
        commit_hive_objective_milestone(tmp.path(), "0.1.0", "hive/worker/x", id, "rust", "main")
            .unwrap();
        let body = fs::read_to_string(tmp.path().join("HIVE_OBJECTIVE.md")).unwrap();
        assert!(body.contains("Objetivo operativo"));
        assert!(body.contains("rust"));
        assert!(body.contains(&id.to_string()));
    }

    #[test]
    fn hive_objective_second_identical_line_skips_commit() {
        let tmp = tempfile::tempdir().unwrap();
        colonize_and_analyze(tmp.path()).unwrap();
        ensure_mainline_seed(tmp.path()).unwrap();
        let id = Uuid::new_v4();
        commit_hive_objective_milestone(tmp.path(), "0.1.0", "hive/worker/x", id, "rust", "main")
            .unwrap();
        let repo = Repository::open(tmp.path()).unwrap();
        let n1 = repo.revwalk().unwrap().count();
        commit_hive_objective_milestone(tmp.path(), "0.1.0", "hive/worker/x", id, "rust", "main")
            .unwrap();
        let n2 = repo.revwalk().unwrap().count();
        assert_eq!(n1, n2);
    }

    #[test]
    fn worker_artifact_body_incluye_modo() {
        let spec = crate::discovery::SpecialistProfile {
            language_key: "rust".into(),
            prompt_blueprint: "p".into(),
            suggested_tools: vec!["cargo".into()],
            weight: 1.0,
        };
        let id = Uuid::nil();
        let s = worker_artifact_body(id, 2, &spec, "tarea", WorkerBranchMode::OrphanRoot);
        assert!(s.contains("OrphanRoot"));
        assert!(s.contains("tarea"));
    }

    #[test]
    fn system_allows_muy_permisivo() {
        let p = ResourcePolicy {
            max_cpu_percent: 101.0,
            min_available_ram_bytes: 0,
        };
        assert!(system_allows_new_instance(&p));
    }

    #[tokio::test]
    async fn run_worker_queue_vacia_no_panics() {
        let (tx, rx) = council::council_channel(1);
        drop(rx);
        let orch = Orchestrator {
            repo_root: std::path::PathBuf::from("/tmp"),
            council_tx: tx,
            resource_policy: ResourcePolicy::default(),
            profile: empty_profile(),
            max_mr_rejection_attempts: 10,
            run_tests_before_mr: false,
            protect_main: false,
            integration_branch: "main".to_string(),
        };
        let st = std::sync::Arc::new(Mutex::new(HiveState::default()));
        run_worker_queue(orch, vec![], st).await.unwrap();
    }

    #[tokio::test]
    async fn drain_task_queue_alias() {
        let (tx, rx) = council::council_channel(1);
        drop(rx);
        let orch = Orchestrator {
            repo_root: "/tmp".into(),
            council_tx: tx,
            resource_policy: ResourcePolicy::default(),
            profile: empty_profile(),
            max_mr_rejection_attempts: 10,
            run_tests_before_mr: false,
            protect_main: false,
            integration_branch: "main".to_string(),
        };
        let st = std::sync::Arc::new(Mutex::new(HiveState::default()));
        drain_task_queue(orch, vec![], st).await.unwrap();
    }

    #[tokio::test]
    async fn incubacion_end_to_end_derived() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::remove_var("HIVE_BRANCH_MODE");
        crate::discovery::colonize_and_analyze(tmp.path()).unwrap();

        let (council_tx, council_rx) = council::council_channel(64);
        tokio::spawn(council::run_council_loop(
            council_rx,
            Maintainer {
                reject_before_approve: 0,
            },
        ));

        let mut dna = RepoDna {
            extension_histogram: HashMap::new(),
            manifest_hits: vec![],
            debt: Default::default(),
            dominant_languages: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        dna.extension_histogram.insert("rs".into(), 2);
        let spec = deduce_specialists(&dna)
            .into_iter()
            .find(|p| p.language_key == "rust")
            .unwrap();

        let task = WorkerTask {
            id: Uuid::new_v4(),
            title: "test".into(),
            specialist: spec,
            branch_mode: WorkerBranchMode::DerivedFromMain,
            mission_one_liner: String::new(),
            mission_brief: "test".into(),
            work_mode: HiveWorkMode::Build,
        };

        let orch = Orchestrator {
            repo_root: tmp.path().to_path_buf(),
            council_tx,
            resource_policy: ResourcePolicy {
                max_cpu_percent: 101.0,
                min_available_ram_bytes: 0,
            },
            profile: empty_profile(),
            max_mr_rejection_attempts: 10,
            run_tests_before_mr: false,
            protect_main: false,
            integration_branch: "main".to_string(),
        };
        let st = std::sync::Arc::new(Mutex::new(HiveState::default()));
        run_worker_queue(orch, vec![task], st.clone())
            .await
            .unwrap();

        let s = st.lock().await;
        assert!(!s.version_history.is_empty());
    }

    #[tokio::test]
    #[serial]
    async fn obrera_descartada_tras_max_rechazos_vuelve_a_main_y_archiva_log() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::remove_var("HIVE_BRANCH_MODE");
        colonize_and_analyze(tmp.path()).unwrap();
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        fs::write(tmp.path().join("main.rs"), "fn main() {}\n").unwrap();

        let (council_tx, council_rx) = council::council_channel(64);
        tokio::spawn(council::run_council_loop(
            council_rx,
            Maintainer {
                reject_before_approve: 100,
            },
        ));

        let mut dna = RepoDna {
            extension_histogram: HashMap::new(),
            manifest_hits: vec![],
            debt: Default::default(),
            dominant_languages: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        dna.extension_histogram.insert("rs".into(), 2);
        let spec = deduce_specialists(&dna)
            .into_iter()
            .find(|p| p.language_key == "rust")
            .unwrap();

        let task = WorkerTask {
            id: Uuid::new_v4(),
            title: "t".into(),
            specialist: spec,
            branch_mode: WorkerBranchMode::DerivedFromMain,
            mission_one_liner: String::new(),
            mission_brief: "b".into(),
            work_mode: HiveWorkMode::Build,
        };
        let branch = format!("hive/worker/{}", task.id);

        let orch = Orchestrator {
            repo_root: tmp.path().to_path_buf(),
            council_tx,
            resource_policy: ResourcePolicy {
                max_cpu_percent: 101.0,
                min_available_ram_bytes: 0,
            },
            profile: empty_profile(),
            max_mr_rejection_attempts: 3,
            run_tests_before_mr: false,
            protect_main: false,
            integration_branch: "main".to_string(),
        };
        let st = std::sync::Arc::new(Mutex::new(HiveState::default()));
        run_worker_queue(orch, vec![task], st).await.unwrap();

        let repo = Repository::open(tmp.path()).unwrap();
        assert!(
            repo.find_branch(&branch, BranchType::Local).is_err(),
            "rama obrera debió borrarse"
        );
        let log = tmp.path().join(".hive/abandoned_mrs.log");
        assert!(log.exists());
        let body = fs::read_to_string(&log).unwrap();
        assert!(body.contains("máximo de rechazos"));
        let head_ref = repo.head().unwrap();
        let head = head_ref.shorthand().unwrap();
        assert_eq!(head, "main");
    }

    #[tokio::test]
    #[serial]
    async fn obrera_sin_reintentos_si_mr_marcado_obsoleto() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::remove_var("HIVE_BRANCH_MODE");
        colonize_and_analyze(tmp.path()).unwrap();
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        fs::write(tmp.path().join("main.rs"), "fn main() {}\n").unwrap();

        let (council_tx, council_rx) = council::council_channel(64);
        tokio::spawn(council::run_council_loop(
            council_rx,
            Maintainer {
                reject_before_approve: 100,
            },
        ));

        let mut dna = RepoDna {
            extension_histogram: HashMap::new(),
            manifest_hits: vec![],
            debt: Default::default(),
            dominant_languages: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        dna.extension_histogram.insert("rs".into(), 2);
        let spec = deduce_specialists(&dna)
            .into_iter()
            .find(|p| p.language_key == "rust")
            .unwrap();

        let task = WorkerTask {
            id: Uuid::new_v4(),
            title: "t".into(),
            specialist: spec,
            branch_mode: WorkerBranchMode::DerivedFromMain,
            mission_one_liner: String::new(),
            mission_brief: "Cierre [HIVE_OBSOLETE] — ya cubierto.".into(),
            work_mode: HiveWorkMode::Build,
        };
        let branch = format!("hive/worker/{}", task.id);

        let orch = Orchestrator {
            repo_root: tmp.path().to_path_buf(),
            council_tx,
            resource_policy: ResourcePolicy {
                max_cpu_percent: 101.0,
                min_available_ram_bytes: 0,
            },
            profile: empty_profile(),
            max_mr_rejection_attempts: 10,
            run_tests_before_mr: false,
            protect_main: false,
            integration_branch: "main".to_string(),
        };
        let st = std::sync::Arc::new(Mutex::new(HiveState::load_or_new(tmp.path()).unwrap()));
        run_worker_queue(orch, vec![task], st.clone())
            .await
            .unwrap();

        let repo = Repository::open(tmp.path()).unwrap();
        assert!(repo.find_branch(&branch, BranchType::Local).is_err());
        let s = st.lock().await;
        assert!(s
            .decision_tree
            .iter()
            .any(|d| d.decision == "mr_descartado_obsoleto"));
    }

    #[test]
    fn merge_derived_falla_si_hay_conflicto() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        let sig = hive_signature("t", "t@t").unwrap();
        fs::write(tmp.path().join("f.txt"), "base").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("f.txt")).unwrap();
        let t1 = index.write_tree().unwrap();
        let tree = repo.find_tree(t1).unwrap();
        let c0 = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .unwrap();

        repo.branch("worker", &repo.find_commit(c0).unwrap(), false)
            .unwrap();
        repo.set_head("refs/heads/worker").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .unwrap();
        fs::write(tmp.path().join("f.txt"), "side-a").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("f.txt")).unwrap();
        let t2 = index.write_tree().unwrap();
        let tree2 = repo.find_tree(t2).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "w", &tree2, &[&head])
            .unwrap();

        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .unwrap();
        fs::write(tmp.path().join("f.txt"), "side-b").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("f.txt")).unwrap();
        let t3 = index.write_tree().unwrap();
        let tree3 = repo.find_tree(t3).unwrap();
        let head_m = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "m", &tree3, &[&head_m])
            .unwrap();

        let err = merge_derived_worker_into_target(tmp.path(), "worker", "main").unwrap_err();
        assert!(err.to_string().contains("conflicto") || err.to_string().contains("conflict"));
    }

    #[test]
    fn merge_orphan_integra_sin_conflicto() {
        let tmp = tempfile::tempdir().unwrap();
        crate::discovery::colonize_and_analyze(tmp.path()).unwrap();
        let spec = crate::discovery::SpecialistProfile {
            language_key: "rust".into(),
            prompt_blueprint: "p".into(),
            suggested_tools: vec![],
            weight: 1.0,
        };
        worker_commit_on_branch(
            tmp.path(),
            "hive/w1",
            &spec,
            "t",
            Uuid::new_v4(),
            1,
            WorkerBranchMode::OrphanRoot,
            false,
            "main",
        )
        .unwrap();
        merge_orphan_worker_into_target(tmp.path(), "hive/w1", "main").unwrap();
        assert!(tmp.path().join("version.json").exists());
    }

    #[test]
    fn append_feedback_crea_commit() {
        let tmp = tempfile::tempdir().unwrap();
        crate::discovery::colonize_and_analyze(tmp.path()).unwrap();
        ensure_mainline_seed(tmp.path()).unwrap();
        append_feedback_note(tmp.path(), "nota del consejo", 1).unwrap();
        assert!(tmp.path().join("hive_council_feedback.log").exists());
    }

    #[tokio::test]
    async fn read_version_string_ok() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("version.json"), r#"{"version":"9.8.7"}"#).unwrap();
        assert_eq!(read_version_string(tmp.path()).await.unwrap(), "9.8.7");
    }

    #[test]
    fn merge_branch_into_main_derived_ok() {
        let tmp = tempfile::tempdir().unwrap();
        crate::discovery::colonize_and_analyze(tmp.path()).unwrap();
        let spec = crate::discovery::SpecialistProfile {
            language_key: "rust".into(),
            prompt_blueprint: "p".into(),
            suggested_tools: vec![],
            weight: 1.0,
        };
        let wid = Uuid::new_v4();
        worker_commit_on_branch(
            tmp.path(),
            "hive/w2",
            &spec,
            "t",
            wid,
            1,
            WorkerBranchMode::DerivedFromMain,
            false,
            "main",
        )
        .unwrap();
        merge_branch_into_target(
            tmp.path(),
            "hive/w2",
            WorkerBranchMode::DerivedFromMain,
            "main",
        )
        .unwrap();
        bump_version_json(tmp.path(), "main").unwrap();
    }

    #[test]
    fn merge_derived_to_integration_leaves_main_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        crate::discovery::colonize_and_analyze(tmp.path()).unwrap();
        ensure_mainline_seed(tmp.path()).unwrap();
        let repo = Repository::open(tmp.path()).unwrap();
        let main_tip = repo.head().unwrap().peel_to_commit().unwrap().id();

        ensure_integration_branch(tmp.path(), "hive/integration", true).unwrap();

        let spec = crate::discovery::SpecialistProfile {
            language_key: "rust".into(),
            prompt_blueprint: "p".into(),
            suggested_tools: vec![],
            weight: 1.0,
        };
        let wid = Uuid::new_v4();
        worker_commit_on_branch(
            tmp.path(),
            "hive/w_protect",
            &spec,
            "t",
            wid,
            1,
            WorkerBranchMode::DerivedFromMain,
            true,
            "hive/integration",
        )
        .unwrap();
        merge_branch_into_target(
            tmp.path(),
            "hive/w_protect",
            WorkerBranchMode::DerivedFromMain,
            "hive/integration",
        )
        .unwrap();

        let repo = Repository::open(tmp.path()).unwrap();
        let main_after = repo
            .find_branch("main", BranchType::Local)
            .unwrap()
            .get()
            .peel_to_commit()
            .unwrap()
            .id();
        assert_eq!(main_after, main_tip);
        let integ_tip = repo
            .find_branch("hive/integration", BranchType::Local)
            .unwrap()
            .get()
            .peel_to_commit()
            .unwrap()
            .id();
        assert_ne!(integ_tip, main_tip);
    }

    #[test]
    fn worker_branch_mode_default() {
        assert_eq!(
            WorkerBranchMode::default(),
            WorkerBranchMode::DerivedFromMain
        );
    }

    #[tokio::test]
    #[serial]
    async fn hive_orchestrator_start_end_to_end() {
        std::env::remove_var("HIVE_BRANCH_MODE");
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        colonize_and_analyze(tmp.path()).unwrap();
        let profile = RepositoryAnalyzer::new(tmp.path().to_path_buf())
            .analyze()
            .unwrap();
        let mut ho = HiveOrchestrator::new(tmp.path().to_path_buf(), profile);
        let cfg = HiveConfig::default();
        let req = HiveRequest::default();
        let wm = request::effective_work_mode(&req, tmp.path());
        ho.start(&cfg, &req, wm).await.unwrap();
        assert!(tmp.path().join("hive.json").exists());
    }

    #[tokio::test]
    #[serial]
    async fn hive_orchestrator_auto_en_titulos_de_tarea() {
        std::env::remove_var("HIVE_BRANCH_MODE");
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname=\"t_auto\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        colonize_and_analyze(tmp.path()).unwrap();
        let profile = RepositoryAnalyzer::new(tmp.path().to_path_buf())
            .analyze()
            .unwrap();
        let mut ho = HiveOrchestrator::new(tmp.path().to_path_buf(), profile);
        let cfg = HiveConfig {
            maintainer_reject_before_approve: 0,
            ..HiveConfig::default()
        };
        let req = HiveRequest {
            title: "Obj".into(),
            description: "Detalle".into(),
            ..Default::default()
        };
        ho.start(&cfg, &req, HiveWorkMode::Auto).await.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn hive_orchestrator_orphan_branch_env() {
        std::env::set_var("HIVE_BRANCH_MODE", "orphan");
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname=\"t2\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        colonize_and_analyze(tmp.path()).unwrap();
        let profile = RepositoryAnalyzer::new(tmp.path().to_path_buf())
            .analyze()
            .unwrap();
        let mut ho = HiveOrchestrator::new(tmp.path().to_path_buf(), profile);
        let cfg = HiveConfig {
            maintainer_reject_before_approve: 0,
            ..HiveConfig::default()
        };
        let req = HiveRequest::default();
        let wm = request::effective_work_mode(&req, tmp.path());
        ho.start(&cfg, &req, wm).await.unwrap();
        std::env::remove_var("HIVE_BRANCH_MODE");
    }

    #[test]
    fn test_worker_branch_mode_variants() {
        let mode_derived = WorkerBranchMode::DerivedFromMain;
        let mode_orphan = WorkerBranchMode::OrphanRoot;
        assert_eq!(mode_derived, WorkerBranchMode::DerivedFromMain);
        assert_eq!(mode_orphan, WorkerBranchMode::OrphanRoot);
    }

    #[test]
    fn test_task_status_variants() {
        let pending = TaskStatus::Pending;
        let in_progress = TaskStatus::InProgress;
        let under_review = TaskStatus::UnderReview;
        let completed = TaskStatus::Completed;
        let failed = TaskStatus::Failed;

        assert_eq!(pending, TaskStatus::Pending);
        assert_eq!(in_progress, TaskStatus::InProgress);
        assert_eq!(under_review, TaskStatus::UnderReview);
        assert_eq!(completed, TaskStatus::Completed);
        assert_eq!(failed, TaskStatus::Failed);
    }

    #[test]
    fn test_worker_task_fields() {
        let specialist = SpecialistProfile {
            language_key: "rust".into(),
            prompt_blueprint: "You are a Rust developer".into(),
            suggested_tools: vec!["cargo".into()],
            weight: 1.0,
        };
        let task = WorkerTask {
            id: Uuid::new_v4(),
            title: "Implement feature".into(),
            specialist,
            branch_mode: WorkerBranchMode::DerivedFromMain,
            mission_one_liner: "Add new feature".into(),
            mission_brief: "Implement the new feature".into(),
            work_mode: HiveWorkMode::Build,
        };
        assert_eq!(task.branch_mode, WorkerBranchMode::DerivedFromMain);
    }

    #[test]
    fn test_hive_orchestrator_fields() {
        let profile = empty_profile();
        let orch = HiveOrchestrator::new(PathBuf::from("/tmp/test"), profile);
        assert_eq!(orch.target, PathBuf::from("/tmp/test"));
    }
}
