//! Punto de entrada del binario **hive_core**.

use anyhow::{Context, Result};
use hive_core::config::HiveConfig;
use hive_core::request::{self, HiveRequest, ProjectStack};
use hive_core::run_queen_cycle;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

#[derive(Debug)]
struct Cli {
    daemon: bool,
    health_repo: Option<PathBuf>,
    target: PathBuf,
    /// Texto de la solicitud (se guarda en `hive.request.json` en el repo objetivo).
    ask: Option<String>,
    /// Feedback del cliente humano: se escribe en `hive.client_feedback.md` y La Reina lo mezcla en la misión del ciclo.
    revise: Option<String>,
    stack: Option<String>,
    force_scaffold: bool,
}

fn parse_cli() -> Result<Cli> {
    let mut args = std::env::args().skip(1).peekable();
    let mut daemon = HiveConfig::daemon_from_env();
    let mut health_repo: Option<PathBuf> = None;
    let mut positional: Vec<PathBuf> = Vec::new();
    let mut ask: Option<String> = None;
    let mut revise: Option<String> = None;
    let mut stack: Option<String> = None;
    let mut force_scaffold = false;

    while let Some(a) = args.next() {
        match a.as_str() {
            "--daemon" | "-d" => daemon = true,
            "--once" => daemon = false,
            "--health" => {
                let p = args.next().context("falta ruta tras --health")?;
                health_repo = Some(PathBuf::from(p));
            }
            "--ask" => {
                let t = args.next().context("falta texto tras --ask")?;
                ask = Some(t);
            }
            "--revise" => {
                let t = args.next().context("falta texto tras --revise")?;
                revise = Some(t);
            }
            "--stack" => {
                let s = args.next().context(
                    "falta valor tras --stack (rust_binary|python_app|node_minimal|auto)",
                )?;
                stack = Some(s);
            }
            "--force-scaffold" => force_scaffold = true,
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            s if s.starts_with('-') => {
                anyhow::bail!("opción desconocida: {s}");
            }
            s => positional.push(PathBuf::from(s)),
        }
    }

    let target = positional
        .pop()
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));

    Ok(Cli {
        daemon,
        health_repo,
        target,
        ask,
        revise,
        stack,
        force_scaffold,
    })
}

fn print_help() {
    eprintln!(
        "\
The Hive (hive_core) — orquestación local con Git y hive.json

Uso:
  hive_core [opciones] [RUTA_REPO]

Opciones:
  -d, --daemon     Bucle continuo (pausa vía HIVE_POLL_INTERVAL_SECS o 10 s)
      --once       Un solo ciclo (anula HIVE_DAEMON)
      --health R   Sale 0 si existe R/hive.json (healthcheck Docker)
      --ask TEXTO  Guarda la solicitud en RUTA/hive.request.json y, si la carpeta
                   está vacía (sin manifest), genera un proyecto mínimo acorde.
      --revise TEXTO  Escribe RUTA/hive.client_feedback.md: tras revisar el repo, el
                   cliente pide cambios; La Reina integra el texto en la misión y
                   vuelve a desplegar la colmena (obreras + Consejo → main).
      --stack KIND rust_binary | python_app | node_minimal | auto (default auto)
      --force-scaffold  Crear esqueleto aunque haya otros ficheros (si no hay manifest)
  -h, --help       Esta ayuda

Variables de entorno:
  HIVE_DAEMON, HIVE_POLL_INTERVAL_SECS, HIVE_MAX_DECISION_RECORDS,
  HIVE_MAX_VERSION_HISTORY, HIVE_MAINTAINER_REJECT_BEFORE_APPROVE, RUST_LOG
  HIVE_REQUEST     Texto de solicitud si no existe hive.request.json
  HIVE_CLIENT_FEEDBACK  Texto de revisión del cliente (misma semántica que hive.client_feedback.md)
  HIVE_REQUEST_JSON  JSON completo de HiveRequest
  HIVE_STACK       rust_binary | python_app | node_minimal | auto
  HIVE_SKIP_RESOURCE_GATE=1  No esperar a bajar CPU/RAM antes de incubar obreras (útil en tests / máquinas cargadas)
  HIVE_MAX_MR_REJECTION_ATTEMPTS  Tras N rechazos del Consejo se descarta la rama obrera (default 10; evita bucles infinitos). Si el Mantenedor indica que el cambio es obsoleto o ya no se requiere, no hay más reintentos.
  HIVE_RUN_TESTS_BEFORE_MR=1  En repos con Cargo.toml ejecuta cargo test antes de enviar el MR al Consejo

  (binario compilado con --features multiagent)
  OLLAMA_BASE_URL  URL del API Ollama (ej. http://127.0.0.1:11434). Sin esta variable se usa un mock.
  OLLAMA_MODEL     Nombre del modelo en Ollama (default codellama). Ej. qwen2.5-coder:7b
  HIVE_USE_MOCK_LLM=1  Forzar respuesta simulada aunque OLLAMA_BASE_URL esté definida

  Docker LLM (Colmena con Ollama): docker compose --profile llm up -d --build  (servicio hive-colmena en bucle)
  Un ciclo: ./scripts/run_llm_docker.sh  o  docker compose --profile llm --profile llm-once run --rm --build hive-llm --once RUTA
  El servicio `rust-toolchain-tests` (perfil dev) no usa Ollama.

hive.request.json (además de title, description, stack):
  work_mode: auto | build | improve
    auto — sin pedido: solo revisión/mejora; con pedido y manifest existente: mejora;
           con pedido sin manifest: construcción (greenfield).
    build / improve — forzar modo para todos los especialistas del ciclo.
"
    );
}

fn run_healthcheck(repo: &Path) -> ! {
    let ok = hive_core::state::HiveState::path_for_repo(repo).exists();
    std::process::exit(if ok { 0 } else { 1 });
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = parse_cli()?;

    if let Some(ref repo) = cli.health_repo {
        run_healthcheck(repo);
    }

    let cfg = HiveConfig::from_env();
    cfg.validate()
        .context("configuración Hive (HIVE_PROTECT_MAIN / HIVE_INTEGRATION_BRANCH)")?;
    let target = cli.target.clone();

    if let Some(ref description) = cli.ask {
        let st = cli
            .stack
            .as_deref()
            .and_then(request::parse_stack)
            .unwrap_or(ProjectStack::Auto);
        let req = HiveRequest {
            title: request::derive_title(description),
            description: description.clone(),
            stack: st,
            force_scaffold: cli.force_scaffold,
            ..Default::default()
        };
        let path = target.join(request::FILENAME);
        fs::write(
            &path,
            serde_json::to_string_pretty(&req).context("serializar hive.request.json")?,
        )
        .with_context(|| format!("escribir {}", path.display()))?;
        info!(path = %path.display(), "solicitud registrada (--ask)");
    }

    if let Some(ref text) = cli.revise {
        let path = target.join(request::CLIENT_FEEDBACK_FILENAME);
        fs::write(&path, text).with_context(|| format!("escribir {}", path.display()))?;
        info!(path = %path.display(), "feedback del cliente registrado (--revise)");
    }

    info!(path = %target.display(), daemon = cli.daemon, "The Hive — inicio");

    if cli.daemon {
        loop {
            run_queen_cycle(target.clone(), &cfg).await.unwrap_or_else(
                |e| tracing::error!(error = %e, "ciclo falló; reintento tras pausa"),
            );
            info!(
                secs = cfg.poll_interval.as_secs(),
                "pausa entre ciclos (daemon)"
            );
            tokio::time::sleep(cfg.poll_interval).await;
        }
    } else {
        run_queen_cycle(target.clone(), &cfg).await?;
        println!(
            "The Queen — ciclo completado. Estado persistido en `{}`.",
            target.join("hive.json").display()
        );
    }

    Ok(())
}
