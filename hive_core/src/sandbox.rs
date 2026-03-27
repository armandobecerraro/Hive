//! Sandbox aislado por obrera usando Docker (bollard).
//!
//! Cada WorkerTask se ejecuta en un contenedor efímero con el repo montado como volumen.
//! Basado en el patrón de OpenHands/OpenDevin.

#[cfg(feature = "multiagent")]
use anyhow::{Context, Result};
#[cfg(feature = "multiagent")]
use bollard::container::{
    Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions,
    WaitContainerOptions,
};
#[cfg(feature = "multiagent")]
use bollard::image::CreateImageOptions;
#[cfg(feature = "multiagent")]
use bollard::Docker;
#[cfg(feature = "multiagent")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "multiagent")]
use std::path::Path;
#[cfg(feature = "multiagent")]
use tracing::info;
#[cfg(feature = "multiagent")]
use futures_util::StreamExt;

/// Configuración del sandbox por obrera.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Imagen Docker base (default: rust:latest).
    pub image: String,
    /// Memoria máxima en bytes (default: 512MB).
    pub memory_limit: i64,
    /// CPUs máximos (default: 1).
    pub cpu_limit: f64,
    /// Timeout en segundos (default: 600).
    pub timeout_secs: u64,
    /// Variables de entorno inyectadas.
    pub env_vars: Vec<String>,
    /// Red del contenedor.
    pub network: String,
}

#[cfg(feature = "multiagent")]
impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            image: "rust:latest".to_string(),
            memory_limit: 512 * 1024 * 1024,
            cpu_limit: 1.0,
            timeout_secs: 600,
            env_vars: vec![
                "CARGO_HOME=/usr/local/cargo".into(),
                "RUST_BACKTRACE=1".into(),
            ],
            network: "bridge".to_string(),
        }
    }
}

/// Resultado de la ejecución en sandbox.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxResult {
    pub container_id: String,
    pub exit_code: i64,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

/// Ejecuta un comando dentro de un contenedor Docker aislado.
#[cfg(feature = "multiagent")]
pub async fn run_in_sandbox(
    repo_root: &Path,
    command: Vec<&str>,
    config: &SandboxConfig,
) -> Result<SandboxResult> {
    let docker = Docker::connect_with_local_defaults()
        .context("conectar al daemon Docker (¿está corriendo?)")?;

    // Verificar conexión
    docker
        .ping()
        .await
        .context("Docker daemon no responde (¿dockerd corriendo?)")?;

    // Pull de la imagen si no existe
    info!(image = %config.image, "verificando imagen Docker");
    let pull_opts = CreateImageOptions {
        from_image: config.image.as_str(),
        ..Default::default()
    };
    let mut pull_stream = docker.create_image(Some(pull_opts), None, None);
    while let Some(item) = pull_stream.next().await {
        let _ = item.context("pull de imagen")?;
    }

    // Crear contenedor
    let abs_repo = std::fs::canonicalize(repo_root)?;
    let container_name = format!("hive-worker-{}", uuid::Uuid::new_v4().simple());

    let host_config = bollard::service::HostConfig {
        binds: Some(vec![format!("{}:/workspace:rw", abs_repo.display())]),
        memory: Some(config.memory_limit),
        nano_cpus: Some((config.cpu_limit * 1e9) as i64),
        network_mode: Some(config.network.clone()),
        readonly_rootfs: Some(false),
        ..Default::default()
    };

    let container_config = Config {
        image: Some(config.image.as_str()),
        cmd: Some(command),
        working_dir: Some("/workspace"),
        env: Some(
            config
                .env_vars
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<&str>>(),
        ),
        host_config: Some(host_config),
        ..Default::default()
    };

    info!(name = %container_name, "creando contenedor sandbox");
    let container = docker
        .create_container(
            Some(CreateContainerOptions {
                name: &container_name,
                platform: None,
            }),
            container_config,
        )
        .await
        .context("crear contenedor")?;

    let container_id = &container.id;
    info!(id = %container_id, "iniciando contenedor");

    docker
        .start_container(container_id, None::<StartContainerOptions<String>>)
        .await
        .context("iniciar contenedor")?;

    // Esperar con timeout
    let wait_result =
        tokio::time::timeout(std::time::Duration::from_secs(config.timeout_secs), async {
            let mut stream =
                docker.wait_container(container_id, None::<WaitContainerOptions<String>>);
            stream.next().await
        })
        .await;

    let (exit_code, timed_out) = match wait_result {
        Ok(Some(Ok(status))) => (status.status_code, false),
        Ok(Some(Err(e))) => {
            info!(error = %e, "error esperando contenedor");
            (-1, false)
        }
        Ok(None) => (-1, false),
        Err(_) => {
            info!(id = %container_id, "timeout: matando contenedor");
            let _ = docker
                .kill_container::<String>(container_id, None::<bollard::container::KillContainerOptions<String>>)
                .await;
            (-1, true)
        }
    };

    // Obtener logs
    let logs_opts = bollard::container::LogsOptions::<String> {
        stdout: true,
        stderr: true,
        ..Default::default()
    };
    let mut log_stream = docker.logs(container_id, Some(logs_opts));
    let mut stdout = String::new();
    let mut stderr = String::new();
    while let Some(Ok(output)) = log_stream.next().await {
        match output {
            bollard::container::LogOutput::StdOut { message } => {
                stdout.push_str(&String::from_utf8_lossy(&message));
            }
            bollard::container::LogOutput::StdErr { message } => {
                stderr.push_str(&String::from_utf8_lossy(&message));
            }
            _ => {}
        }
    }

    // Limpiar contenedor
    let _ = docker
        .remove_container(
            container_id,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await;

    Ok(SandboxResult {
        container_id: container_id.clone(),
        exit_code,
        stdout,
        stderr,
        timed_out,
    })
}

/// Ejecuta cargo test en sandbox.
#[cfg(feature = "multiagent")]
pub async fn run_cargo_test_sandbox(repo_root: &Path) -> Result<SandboxResult> {
    let config = SandboxConfig {
        image: "rust:latest".to_string(),
        ..Default::default()
    };
    run_in_sandbox(
        repo_root,
        vec!["cargo", "test", "--no-fail-fast", "-q"],
        &config,
    )
    .await
}

/// Ejecuta un script arbitrario en sandbox.
#[cfg(feature = "multiagent")]
pub async fn run_script_sandbox(
    repo_root: &Path,
    script: &str,
    config: &SandboxConfig,
) -> Result<SandboxResult> {
    run_in_sandbox(repo_root, vec!["sh", "-c", script], config).await
}

// Stubs cuando la feature no está activada
#[cfg(not(feature = "multiagent"))]
pub struct SandboxResult {
    pub exit_code: i64,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

#[cfg(not(feature = "multiagent"))]
pub async fn run_cargo_test_sandbox(_repo_root: &std::path::Path) -> anyhow::Result<SandboxResult> {
    anyhow::bail!("feature 'multiagent' requerida para sandbox Docker")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "multiagent")]
    #[tokio::test]
    async fn sandbox_config_defaults() {
        let cfg = SandboxConfig::default();
        assert_eq!(cfg.image, "rust:latest");
        assert_eq!(cfg.memory_limit, 512 * 1024 * 1024);
        assert_eq!(cfg.timeout_secs, 600);
    }
}
