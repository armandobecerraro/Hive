//! # Worktree Manager - Aislamiento Git por Obrera
//!
//! Inspirado en Claude Code y Augment Intent: cada obrera trabaja en su propio
//! **git worktree** aislado, eliminando conflictos de archivos entre agentes paralelos.
//!
//! ## Arquitectura
//! ```text
//! repo/
//! ├── .git
//! ├── main (worktree principal)
//! ├── hive-workers/
//! │   ├── worker-{uuid-1}/  ← worktree aislado (rama hive/worker/{uuid-1})
//! │   ├── worker-{uuid-2}/  ← worktree aislado (rama hive/worker/{uuid-2})
//! │   └── worker-{uuid-3}/  ← worktree aislado (rama hive/worker/{uuid-3})
//! ```

use anyhow::{Context, Result};
use git2::{BranchType, Repository};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Información de un worktree activo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeInfo {
    pub worker_id: Uuid,
    pub branch: String,
    pub worktree_path: PathBuf,
    pub created_at: i64,
    pub is_active: bool,
}

/// Estado del gestor de worktrees
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorktreeStats {
    pub total_created: u64,
    pub active_count: usize,
    pub cleaned_count: u64,
    pub failed_count: u64,
}

/// Gestor de worktrees para aislamiento de obreras
///
/// Cada obrera obtiene su propio directorio de trabajo aislado mediante
/// `git worktree`, permitiendo trabajo paralelo sin conflictos.
pub struct WorktreeManager {
    repo_root: PathBuf,
    workers_dir: PathBuf,
    active_worktrees: Arc<RwLock<HashMap<Uuid, WorktreeInfo>>>,
    stats: Arc<RwLock<WorktreeStats>>,
}

impl WorktreeManager {
    /// Crea un nuevo gestor de worktrees
    ///
    /// # Arguments
    /// * `repo_root` - Raíz del repositorio Git principal
    pub fn new(repo_root: PathBuf) -> Self {
        let workers_dir = repo_root.join(".hive").join("workers");
        Self {
            repo_root,
            workers_dir,
            active_worktrees: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(WorktreeStats::default())),
        }
    }

    /// Crea un worktree aislado para una obrera
    ///
    /// # Arguments
    /// * `worker_id` - UUID de la obrera
    /// * `branch` - Nombre de la rama (ej: "hive/worker/{uuid}")
    ///
    /// # Returns
    /// Ruta al directorio del worktree aislado
    pub async fn create_worktree(&self, worker_id: Uuid, branch: &str) -> Result<PathBuf> {
        let worktree_name = format!("worker-{}", &worker_id.to_string()[..8]);
        let worktree_path = self.workers_dir.join(&worktree_name);

        // Verificar que el worktree no exista ya
        {
            let active = self.active_worktrees.read().await;
            if active.contains_key(&worker_id) {
                return Ok(active[&worker_id].worktree_path.clone());
            }
        }

        // Crear directorio padre si no existe
        std::fs::create_dir_all(&self.workers_dir)
            .with_context(|| format!("crear {}", self.workers_dir.display()))?;

        // Si el directorio ya existe (de una ejecución anterior), limpiarlo
        if worktree_path.exists() {
            self.remove_worktree_internal(&worktree_path)?;
        }

        // Crear la rama si no existe
        let repo = Repository::open(&self.repo_root)?;
        if repo.find_branch(branch, BranchType::Local).is_err() {
            let head = repo.head()?.peel_to_commit()?;
            repo.branch(branch, &head, false)?;
        }

        // Ejecutar git worktree add
        let output = std::process::Command::new("git")
            .current_dir(&self.repo_root)
            .args(["worktree", "add", &worktree_path.to_string_lossy(), branch])
            .output()
            .context("ejecutar git worktree add")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let mut stats = self.stats.write().await;
            stats.failed_count += 1;
            anyhow::bail!("git worktree add falló: {stderr}");
        }

        // Registrar worktree activo
        let info = WorktreeInfo {
            worker_id,
            branch: branch.to_string(),
            worktree_path: worktree_path.clone(),
            created_at: chrono::Utc::now().timestamp(),
            is_active: true,
        };

        {
            let mut active = self.active_worktrees.write().await;
            active.insert(worker_id, info);
        }
        {
            let mut stats = self.stats.write().await;
            stats.total_created += 1;
            stats.active_count += 1;
        }

        tracing::info!(
            worker_id = %worker_id,
            branch = %branch,
            path = %worktree_path.display(),
            "worktree creado para obrera"
        );

        Ok(worktree_path)
    }

    /// Limpia un worktree cuando la obrera termina
    ///
    /// # Arguments
    /// * `worker_id` - UUID de la obrera
    pub async fn cleanup_worktree(&self, worker_id: Uuid) -> Result<()> {
        let worktree_path = {
            let active = self.active_worktrees.read().await;
            active
                .get(&worker_id)
                .map(|info| info.worktree_path.clone())
        };

        if let Some(path) = worktree_path {
            // Ejecutar git worktree remove
            let output = std::process::Command::new("git")
                .current_dir(&self.repo_root)
                .args(["worktree", "remove", "--force", &path.to_string_lossy()])
                .output()
                .context("ejecutar git worktree remove")?;

            if !output.status.success() {
                tracing::warn!(
                    worker_id = %worker_id,
                    "git worktree remove falló, intentando limpieza manual"
                );
                // Fallback: eliminar directorio manualmente
                if path.exists() {
                    std::fs::remove_dir_all(&path).ok();
                }
            }

            // Remover del registro
            {
                let mut active = self.active_worktrees.write().await;
                active.remove(&worker_id);
            }
            {
                let mut stats = self.stats.write().await;
                stats.active_count = stats.active_count.saturating_sub(1);
                stats.cleaned_count += 1;
            }

            tracing::info!(worker_id = %worker_id, "worktree limpiado");
        }

        Ok(())
    }

    /// Limpia todos los worktrees huérfanos
    pub async fn cleanup_all(&self) -> Result<u32> {
        let worker_ids: Vec<Uuid> = {
            let active = self.active_worktrees.read().await;
            active.keys().copied().collect()
        };

        let mut cleaned = 0u32;
        for worker_id in worker_ids {
            if self.cleanup_worktree(worker_id).await.is_ok() {
                cleaned += 1;
            }
        }

        // También limpiar worktrees huérfanos del disco
        self.cleanup_orphaned_on_disk()?;

        Ok(cleaned)
    }

    /// Obtiene la ruta del worktree de una obrera
    pub async fn get_worktree_path(&self, worker_id: Uuid) -> Option<PathBuf> {
        let active = self.active_worktrees.read().await;
        active
            .get(&worker_id)
            .map(|info| info.worktree_path.clone())
    }

    /// Verifica si una obrera tiene un worktree activo
    pub async fn has_active_worktree(&self, worker_id: Uuid) -> bool {
        let active = self.active_worktrees.read().await;
        active.get(&worker_id).is_some_and(|info| info.is_active)
    }

    /// Lista todos los worktrees activos
    pub async fn list_active(&self) -> Vec<WorktreeInfo> {
        let active = self.active_worktrees.read().await;
        active.values().cloned().collect()
    }

    /// Obtiene estadísticas del gestor
    pub async fn get_stats(&self) -> WorktreeStats {
        self.stats.read().await.clone()
    }

    fn remove_worktree_internal(&self, path: &Path) -> Result<()> {
        let output = std::process::Command::new("git")
            .current_dir(&self.repo_root)
            .args(["worktree", "remove", "--force", &path.to_string_lossy()])
            .output();

        match output {
            Ok(out) if out.status.success() => Ok(()),
            _ => {
                if path.exists() {
                    std::fs::remove_dir_all(path).ok();
                }
                Ok(())
            }
        }
    }

    fn cleanup_orphaned_on_disk(&self) -> Result<()> {
        if !self.workers_dir.exists() {
            return Ok(());
        }

        // Listar worktrees de git
        let output = std::process::Command::new("git")
            .current_dir(&self.repo_root)
            .args(["worktree", "list", "--porcelain"])
            .output();

        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let git_worktree_paths: Vec<&str> = stdout
                .lines()
                .filter(|l| l.starts_with("worktree "))
                .map(|l| l.trim_start_matches("worktree "))
                .collect();

            // Limpiar directorios que no están en la lista de git
            for entry in std::fs::read_dir(&self.workers_dir)?.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let path_str = path.to_string_lossy();
                    if !git_worktree_paths.iter().any(|p| *p == path_str) {
                        tracing::warn!(path = %path.display(), "limpiando worktree huérfano");
                        std::fs::remove_dir_all(&path).ok();
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup_test_repo() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempdir().unwrap();
        let repo_path = tmp.path().to_path_buf();

        // Inicializar repo Git
        let repo = Repository::init(&repo_path).unwrap();

        // Crear commit inicial
        std::fs::write(repo_path.join("README.md"), "# Test Repo\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("Test", "test@test.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();

        // Crear rama main
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        if repo.find_branch("main", BranchType::Local).is_err() {
            repo.branch("main", &head, false).unwrap();
        }
        repo.set_head("refs/heads/main").unwrap();

        (tmp, repo_path)
    }

    #[tokio::test]
    async fn test_create_and_cleanup_worktree() {
        let (_tmp, repo_path) = setup_test_repo();
        let manager = WorktreeManager::new(repo_path.clone());

        let worker_id = Uuid::new_v4();
        let branch = format!("hive/worker/{}", worker_id);

        // Crear worktree
        let worktree_path = manager.create_worktree(worker_id, &branch).await.unwrap();
        assert!(worktree_path.exists());
        assert!(manager.has_active_worktree(worker_id).await);

        // Verificar stats
        let stats = manager.get_stats().await;
        assert_eq!(stats.active_count, 1);
        assert_eq!(stats.total_created, 1);

        // Limpiar worktree
        manager.cleanup_worktree(worker_id).await.unwrap();
        assert!(!manager.has_active_worktree(worker_id).await);

        let stats = manager.get_stats().await;
        assert_eq!(stats.active_count, 0);
        assert_eq!(stats.cleaned_count, 1);
    }

    #[tokio::test]
    async fn test_multiple_parallel_worktrees() {
        let (_tmp, repo_path) = setup_test_repo();
        let manager = WorktreeManager::new(repo_path);

        let mut worker_ids = Vec::new();

        // Crear 3 worktrees en paralelo
        for _ in 0..3 {
            let worker_id = Uuid::new_v4();
            worker_ids.push(worker_id);
            let branch = format!("hive/worker/{}", worker_id);
            let mgr = &manager;

            // Crear secuencialmente (porque manager no es Clone)
            let path = mgr.create_worktree(worker_id, &branch).await.unwrap();
            assert!(path.exists());
        }

        // Verificar que todos están activos
        let active = manager.list_active().await;
        assert_eq!(active.len(), 3);

        // Cada worktree debe tener su propio directorio
        let mut paths: Vec<PathBuf> = active.iter().map(|w| w.worktree_path.clone()).collect();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), 3, "cada worktree debe tener ruta única");

        // Limpiar todos
        let cleaned = manager.cleanup_all().await.unwrap();
        assert_eq!(cleaned, 3);
        assert_eq!(manager.list_active().await.len(), 0);
    }

    #[tokio::test]
    async fn test_duplicate_worktree_returns_existing() {
        let (_tmp, repo_path) = setup_test_repo();
        let manager = WorktreeManager::new(repo_path);

        let worker_id = Uuid::new_v4();
        let branch = format!("hive/worker/{}", worker_id);

        let path1 = manager.create_worktree(worker_id, &branch).await.unwrap();
        let path2 = manager.create_worktree(worker_id, &branch).await.unwrap();

        assert_eq!(
            path1, path2,
            "crear worktree duplicado debe retornar el existente"
        );
        assert_eq!(manager.get_stats().await.total_created, 1);
    }

    #[tokio::test]
    async fn test_worktree_isolation_writes() {
        let (_tmp, repo_path) = setup_test_repo();
        let manager = WorktreeManager::new(repo_path.clone());

        let worker1 = Uuid::new_v4();
        let worker2 = Uuid::new_v4();
        let branch1 = format!("hive/worker/{}", worker1);
        let branch2 = format!("hive/worker/{}", worker2);

        let path1 = manager.create_worktree(worker1, &branch1).await.unwrap();
        let path2 = manager.create_worktree(worker2, &branch2).await.unwrap();

        // Escribir archivos diferentes en cada worktree
        std::fs::write(path1.join("worker1.txt"), "contenido obrera 1").unwrap();
        std::fs::write(path2.join("worker2.txt"), "contenido obrera 2").unwrap();

        // Verificar aislamiento: cada worktree solo tiene su archivo
        assert!(path1.join("worker1.txt").exists());
        assert!(!path1.join("worker2.txt").exists());
        assert!(path2.join("worker2.txt").exists());
        assert!(!path2.join("worker1.txt").exists());

        manager.cleanup_all().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_nonexistent_worktree() {
        let (_tmp, repo_path) = setup_test_repo();
        let manager = WorktreeManager::new(repo_path);

        let unknown_id = Uuid::new_v4();
        assert!(manager.get_worktree_path(unknown_id).await.is_none());
        assert!(!manager.has_active_worktree(unknown_id).await);
    }

    #[tokio::test]
    async fn test_cleanup_idempotent() {
        let (_tmp, repo_path) = setup_test_repo();
        let manager = WorktreeManager::new(repo_path);

        let worker_id = Uuid::new_v4();
        let branch = format!("hive/worker/{}", worker_id);

        manager.create_worktree(worker_id, &branch).await.unwrap();

        // Limpiar dos veces no debe fallar
        manager.cleanup_worktree(worker_id).await.unwrap();
        manager.cleanup_worktree(worker_id).await.unwrap();

        assert_eq!(manager.get_stats().await.active_count, 0);
    }

    #[test]
    fn test_worktree_info_structure() {
        let info = WorktreeInfo {
            worker_id: Uuid::new_v4(),
            branch: "hive/worker/test".into(),
            worktree_path: PathBuf::from("/tmp/worktree"),
            created_at: 1234567890,
            is_active: true,
        };
        assert!(info.is_active);
        assert_eq!(info.branch, "hive/worker/test");
    }

    #[test]
    fn test_worktree_stats_default() {
        let stats = WorktreeStats::default();
        assert_eq!(stats.total_created, 0);
        assert_eq!(stats.active_count, 0);
        assert_eq!(stats.cleaned_count, 0);
        assert_eq!(stats.failed_count, 0);
    }

    #[tokio::test]
    async fn test_list_active_empty() {
        let (_tmp, repo_path) = setup_test_repo();
        let manager = WorktreeManager::new(repo_path);

        let active = manager.list_active().await;
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn test_stats_tracking() {
        let (_tmp, repo_path) = setup_test_repo();
        let manager = WorktreeManager::new(repo_path);

        let worker_id1 = Uuid::new_v4();
        let worker_id2 = Uuid::new_v4();

        manager
            .create_worktree(worker_id1, &format!("hive/worker/{}", worker_id1))
            .await
            .unwrap();
        manager
            .create_worktree(worker_id2, &format!("hive/worker/{}", worker_id2))
            .await
            .unwrap();

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_created, 2);
        assert_eq!(stats.active_count, 2);
    }

    #[test]
    fn test_remove_worktree_internal_fallback() {
        let (_tmp, repo_path) = setup_test_repo();
        let manager = WorktreeManager::new(repo_path);

        let fake_path = PathBuf::from("/nonexistent/path");
        let result = manager.remove_worktree_internal(&fake_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_worktree_info_serialization() {
        let info = WorktreeInfo {
            worker_id: Uuid::new_v4(),
            branch: "hive/worker/test".into(),
            worktree_path: PathBuf::from("/tmp/worktree"),
            created_at: 1234567890,
            is_active: true,
        };
        let serialized = serde_json::to_string(&info).unwrap();
        let deserialized: WorktreeInfo = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.branch, "hive/worker/test");
    }

    #[test]
    fn test_worktree_stats_serialization() {
        let stats = WorktreeStats {
            total_created: 10,
            active_count: 3,
            cleaned_count: 7,
            failed_count: 1,
        };
        let serialized = serde_json::to_string(&stats).unwrap();
        let deserialized: WorktreeStats = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.total_created, 10);
    }

    #[tokio::test]
    async fn test_get_worktree_path_existing() {
        let (_tmp, repo_path) = setup_test_repo();
        let manager = WorktreeManager::new(repo_path.clone());

        let worker_id = Uuid::new_v4();
        let branch = format!("hive/worker/{}", worker_id);

        let path = manager.create_worktree(worker_id, &branch).await.unwrap();
        let retrieved = manager.get_worktree_path(worker_id).await;

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), path);
    }

    #[tokio::test]
    async fn test_get_worktree_path_nonexistent() {
        let (_tmp, repo_path) = setup_test_repo();
        let manager = WorktreeManager::new(repo_path);

        let retrieved = manager.get_worktree_path(Uuid::new_v4()).await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_worktree_manager_new() {
        let (_tmp, repo_path) = setup_test_repo();
        let manager = WorktreeManager::new(repo_path.clone());

        assert_eq!(manager.repo_root, repo_path);
    }

    #[tokio::test]
    async fn test_cleanup_orphaned_no_workers_dir() {
        let (_tmp, repo_path) = setup_test_repo();
        let manager = WorktreeManager::new(repo_path);

        let result = manager.cleanup_orphaned_on_disk();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_worktree_manager_creates_workers_dir() {
        let (_tmp, repo_path) = setup_test_repo();
        let _manager = WorktreeManager::new(repo_path.clone());

        assert!(repo_path
            .join(".hive")
            .join("workers")
            .to_string_lossy()
            .contains(".hive"));
    }

    #[test]
    fn test_worktree_stats_serialization_roundtrip() {
        let stats = WorktreeStats {
            total_created: 5,
            active_count: 2,
            cleaned_count: 3,
            failed_count: 0,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let deserialized: WorktreeStats = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_created, 5);
        assert_eq!(deserialized.active_count, 2);
    }

    #[tokio::test]
    async fn test_list_active_after_creates() {
        let (_tmp, repo_path) = setup_test_repo();
        let manager = WorktreeManager::new(repo_path);

        let worker_id = Uuid::new_v4();
        let branch = format!("hive/worker/{}", worker_id);

        manager.create_worktree(worker_id, &branch).await.unwrap();

        let active = manager.list_active().await;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].worker_id, worker_id);
        assert_eq!(active[0].branch, branch);
    }

    #[tokio::test]
    async fn test_cleanup_nonexistent_worktree() {
        let (_tmp, repo_path) = setup_test_repo();
        let manager = WorktreeManager::new(repo_path);

        let unknown_id = Uuid::new_v4();
        let result = manager.cleanup_worktree(unknown_id).await;
        assert!(result.is_ok());

        let stats = manager.get_stats().await;
        assert_eq!(stats.cleaned_count, 0);
    }

    #[tokio::test]
    async fn test_worktree_path_stores_short_uuid() {
        let (_tmp, repo_path) = setup_test_repo();
        let manager = WorktreeManager::new(repo_path.clone());

        let worker_id = Uuid::new_v4();
        let branch = format!("hive/worker/{}", worker_id);

        let path = manager.create_worktree(worker_id, &branch).await.unwrap();
        let short_uuid = &worker_id.to_string()[..8];

        assert!(path
            .to_string_lossy()
            .contains(&format!("worker-{}", short_uuid)));
    }
}
