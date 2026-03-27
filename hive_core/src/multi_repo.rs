//! Soporte multi-repo — orquestación sobre múltiples repositorios.
//!
//! Una sola Queen puede orquestar tareas sobre múltiples repos en paralelo,
//! cada uno con su propio perfil ADN, sus propias obreras y su propio Consejo.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::info;

/// Configuración de un repo individual en un enjambre multi-repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub path: PathBuf,
    pub name: String,
    pub enabled: bool,
    pub priority: u32,
    pub stack_override: Option<String>,
    pub labels: Vec<String>,
}

/// Colección de repos a orquestar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiRepoWorkspace {
    pub name: String,
    pub repos: Vec<RepoConfig>,
    pub global_config: HashMap<String, String>,
}

impl MultiRepoWorkspace {
    /// Carga desde un fichero hive.workspace.json.
    pub fn load(repo_root: &Path) -> Result<Self> {
        let path = repo_root.join("hive.workspace.json");
        let content =
            std::fs::read_to_string(&path).with_context(|| format!("leer {}", path.display()))?;
        let workspace: MultiRepoWorkspace = serde_json::from_str(&content)?;
        Ok(workspace)
    }

    /// Intenta cargar; si no existe, crea un workspace con un solo repo.
    pub fn load_or_single(repo_root: &Path) -> Result<Self> {
        let path = repo_root.join("hive.workspace.json");
        if path.exists() {
            Self::load(repo_root)
        } else {
            Ok(Self {
                name: "default".into(),
                repos: vec![RepoConfig {
                    path: repo_root.to_path_buf(),
                    name: repo_root
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("repo")
                        .to_string(),
                    enabled: true,
                    priority: 1,
                    stack_override: None,
                    labels: vec![],
                }],
                global_config: HashMap::new(),
            })
        }
    }

    /// Guarda la configuración.
    pub fn save(&self, repo_root: &Path) -> Result<()> {
        let path = repo_root.join("hive.workspace.json");
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    /// Devuelve solo los repos habilitados, ordenados por prioridad.
    pub fn enabled_repos(&self) -> Vec<&RepoConfig> {
        let mut repos: Vec<&RepoConfig> = self.repos.iter().filter(|r| r.enabled).collect();
        repos.sort_by_key(|r| r.priority);
        repos
    }

    /// Filtra repos por label.
    pub fn filter_by_label(&self, label: &str) -> Vec<&RepoConfig> {
        self.repos
            .iter()
            .filter(|r| r.enabled && r.labels.iter().any(|l| l == label))
            .collect()
    }

    /// Número total de repos habilitados.
    pub fn enabled_count(&self) -> usize {
        self.repos.iter().filter(|r| r.enabled).count()
    }
}

/// Resultado de orquestar un repo individual.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoResult {
    pub repo_name: String,
    pub success: bool,
    pub cycles_completed: u32,
    pub workers_spawned: u32,
    pub merges_approved: u32,
    pub merges_rejected: u32,
    pub duration_secs: u64,
    pub error: Option<String>,
}

/// Estadísticas globales del workspace multi-repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiRepoStats {
    pub total_repos: usize,
    pub successful: usize,
    pub failed: usize,
    pub total_cycles: u32,
    pub total_workers: u32,
    pub total_merges: u32,
    pub results: Vec<RepoResult>,
}

/// Orquestador multi-repo.
pub struct MultiRepoOrchestrator {
    workspace: MultiRepoWorkspace,
}

impl MultiRepoOrchestrator {
    pub fn new(workspace: MultiRepoWorkspace) -> Self {
        Self { workspace }
    }

    /// Ejecuta ciclos sobre todos los repos habilitados.
    pub async fn run_all(&self, cfg: &crate::config::HiveConfig) -> Result<MultiRepoStats> {
        let repos = self.workspace.enabled_repos();
        info!(count = repos.len(), "orquestación multi-repo iniciada");

        let mut handles = Vec::new();
        for repo in &repos {
            let repo_path = repo.path.clone();
            let repo_name = repo.name.clone();
            let cfg_clone = cfg.clone();

            handles.push(tokio::spawn(async move {
                let start = std::time::Instant::now();
                info!(repo = %repo_name, "iniciando ciclo en repo");
                let result = crate::run_queen_cycle(repo_path, &cfg_clone).await;
                let duration = start.elapsed().as_secs();

                RepoResult {
                    repo_name,
                    success: result.is_ok(),
                    cycles_completed: 1,
                    workers_spawned: 0,
                    merges_approved: 0,
                    merges_rejected: 0,
                    duration_secs: duration,
                    error: result.err().map(|e| e.to_string()),
                }
            }));
        }

        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(r) => results.push(r),
                Err(e) => results.push(RepoResult {
                    repo_name: "unknown".into(),
                    success: false,
                    cycles_completed: 0,
                    workers_spawned: 0,
                    merges_approved: 0,
                    merges_rejected: 0,
                    duration_secs: 0,
                    error: Some(e.to_string()),
                }),
            }
        }

        let successful = results.iter().filter(|r| r.success).count();
        let failed = results.len() - successful;

        Ok(MultiRepoStats {
            total_repos: results.len(),
            successful,
            failed,
            total_cycles: results.iter().map(|r| r.cycles_completed).sum(),
            total_workers: results.iter().map(|r| r.workers_spawned).sum(),
            total_merges: results
                .iter()
                .map(|r| r.merges_approved + r.merges_rejected)
                .sum(),
            results,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_crea_single_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = MultiRepoWorkspace::load_or_single(tmp.path()).unwrap();
        assert_eq!(ws.enabled_count(), 1);
        assert_eq!(
            ws.repos[0].name,
            tmp.path().file_name().unwrap().to_str().unwrap()
        );
    }

    #[test]
    fn workspace_filter_by_label() {
        let ws = MultiRepoWorkspace {
            name: "test".into(),
            repos: vec![
                RepoConfig {
                    path: "/a".into(),
                    name: "a".into(),
                    enabled: true,
                    priority: 1,
                    stack_override: None,
                    labels: vec!["rust".into()],
                },
                RepoConfig {
                    path: "/b".into(),
                    name: "b".into(),
                    enabled: true,
                    priority: 2,
                    stack_override: None,
                    labels: vec!["python".into()],
                },
            ],
            global_config: HashMap::new(),
        };
        assert_eq!(ws.filter_by_label("rust").len(), 1);
        assert_eq!(ws.filter_by_label("python").len(), 1);
        assert_eq!(ws.filter_by_label("go").len(), 0);
    }

    #[test]
    fn workspace_disabled_repos_excluded() {
        let ws = MultiRepoWorkspace {
            name: "test".into(),
            repos: vec![
                RepoConfig {
                    path: "/a".into(),
                    name: "a".into(),
                    enabled: true,
                    priority: 1,
                    stack_override: None,
                    labels: vec![],
                },
                RepoConfig {
                    path: "/b".into(),
                    name: "b".into(),
                    enabled: false,
                    priority: 1,
                    stack_override: None,
                    labels: vec![],
                },
            ],
            global_config: HashMap::new(),
        };
        assert_eq!(ws.enabled_count(), 1);
    }
}
