//! # File Lock Manager - Bloqueo de Hotspots
//!
//! Inspirado en OpenAI Symphony (Elixir/BEAM): previene que múltiples obreras
//! modifiquen simultáneamente archivos críticos como `Cargo.toml`, `README.md`,
//! `.gitignore`, `version.json`, etc.
//!
//! ## Uso
//! ```rust,ignore
//! let locks = FileLockManager::new();
//!
//! // Obrera intenta bloquear Cargo.toml
//! if locks.try_acquire(worker_id, Path::new("Cargo.toml")).await? {
//!     // Trabajar con el archivo...
//!     locks.release(worker_id, Path::new("Cargo.toml")).await;
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Hotspots que requieren bloqueo automático
const DEFAULT_HOTSPOTS: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "requirements.txt",
    "pubspec.yaml",
    "README.md",
    "README",
    ".gitignore",
    "version.json",
    "hive.json",
    "hive.request.json",
    "Makefile",
    "Dockerfile",
    "docker-compose.yml",
    "docker-compose.yaml",
    ".github/workflows",
];

/// Información de un bloqueo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLock {
    pub worker_id: Uuid,
    pub file_path: PathBuf,
    pub acquired_at: i64,
    pub expires_at: Option<i64>,
}

/// Resultado de intento de adquisición
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockResult {
    /// Bloqueo adquirido exitosamente
    Acquired,
    /// Archivo ya bloqueado por otra obrera
    AlreadyLocked { owner: Uuid },
    /// No es un hotspot (no requiere bloqueo)
    NotHotspot,
}

/// Estadísticas del gestor de bloqueos
#[derive(Debug, Clone, Default, Serialize)]
pub struct LockStats {
    pub total_acquired: u64,
    pub total_released: u64,
    pub total_conflicts: u64,
    pub active_locks: usize,
    pub hotspot_checks: u64,
}

/// Gestor de bloqueos de archivos
///
/// Controla el acceso a archivos compartidos para prevenir
/// condiciones de carrera entre obreras paralelas.
pub struct FileLockManager {
    locks: Arc<RwLock<HashMap<PathBuf, FileLock>>>,
    hotspots: Vec<String>,
    stats: Arc<RwLock<LockStats>>,
}

impl FileLockManager {
    /// Crea un nuevo gestor con los hotspots por defecto
    pub fn new() -> Self {
        Self {
            locks: Arc::new(RwLock::new(HashMap::new())),
            hotspots: DEFAULT_HOTSPOTS.iter().map(|s| s.to_string()).collect(),
            stats: Arc::new(RwLock::new(LockStats::default())),
        }
    }

    /// Crea un gestor con hotspots personalizados
    pub fn with_hotspots(hotspots: Vec<String>) -> Self {
        Self {
            locks: Arc::new(RwLock::new(HashMap::new())),
            hotspots,
            stats: Arc::new(RwLock::new(LockStats::default())),
        }
    }

    /// Intenta adquirir un bloqueo sobre un archivo
    ///
    /// # Arguments
    /// * `worker_id` - UUID de la obrera que solicita el bloqueo
    /// * `file_path` - Ruta del archivo (relativa al repo)
    ///
    /// # Returns
    /// `LockResult::Acquired` si se obtuvo el bloqueo
    pub async fn try_acquire(&self, worker_id: Uuid, file_path: &Path) -> Result<LockResult, anyhow::Error> {
        let normalized = self.normalize_path(file_path);

        // Verificar si es hotspot
        if !self.is_hotspot(&normalized) {
            let mut stats = self.stats.write().await;
            stats.hotspot_checks += 1;
            return Ok(LockResult::NotHotspot);
        }

        let mut locks = self.locks.write().await;
        let mut stats = self.stats.write().await;
        stats.hotspot_checks += 1;

        // Verificar si ya está bloqueado
        if let Some(existing) = locks.get(&normalized) {
            if existing.worker_id == worker_id {
                // Ya lo tenemos nosotros
                return Ok(LockResult::Acquired);
            }

            // Verificar si el bloqueo expiró
            if let Some(expires) = existing.expires_at {
                if chrono::Utc::now().timestamp() > expires {
                    // Bloqueo expirado, tomarlo
                    locks.insert(normalized.clone(), FileLock {
                        worker_id,
                        file_path: normalized,
                        acquired_at: chrono::Utc::now().timestamp(),
                        expires_at: None,
                    });
                    stats.total_acquired += 1;
                    stats.active_locks = locks.len();
                    return Ok(LockResult::Acquired);
                }
            }

            stats.total_conflicts += 1;
            return Ok(LockResult::AlreadyLocked {
                owner: existing.worker_id,
            });
        }

        // Adquirir bloqueo
        locks.insert(normalized.clone(), FileLock {
            worker_id,
            file_path: normalized,
            acquired_at: chrono::Utc::now().timestamp(),
            expires_at: None,
        });

        stats.total_acquired += 1;
        stats.active_locks = locks.len();

        Ok(LockResult::Acquired)
    }

    /// Adquiere bloqueo con timeout
    ///
    /// Intenta adquirir el bloqueo reintentando hasta `timeout_secs` segundos.
    pub async fn acquire_with_timeout(
        &self,
        worker_id: Uuid,
        file_path: &Path,
        timeout_secs: u64,
    ) -> Result<bool, anyhow::Error> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

        while std::time::Instant::now() < deadline {
            match self.try_acquire(worker_id, file_path).await? {
                LockResult::Acquired | LockResult::NotHotspot => return Ok(true),
                LockResult::AlreadyLocked { .. } => {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }

        Ok(false)
    }

    /// Libera un bloqueo
    pub async fn release(&self, worker_id: Uuid, file_path: &Path) -> bool {
        let normalized = self.normalize_path(file_path);
        let mut locks = self.locks.write().await;

        if let Some(lock) = locks.get(&normalized) {
            if lock.worker_id == worker_id {
                locks.remove(&normalized);
                let mut stats = self.stats.write().await;
                stats.total_released += 1;
                stats.active_locks = locks.len();
                return true;
            }
        }

        false
    }

    /// Libera todos los bloqueos de una obrera
    pub async fn release_all(&self, worker_id: Uuid) -> u32 {
        let mut locks = self.locks.write().await;
        let to_remove: Vec<PathBuf> = locks
            .iter()
            .filter(|(_, lock)| lock.worker_id == worker_id)
            .map(|(path, _)| path.clone())
            .collect();

        let count = to_remove.len() as u32;
        for path in to_remove {
            locks.remove(&path);
        }

        let mut stats = self.stats.write().await;
        stats.total_released += count as u64;
        stats.active_locks = locks.len();

        count
    }

    /// Verifica si un archivo está bloqueado
    pub async fn is_locked(&self, file_path: &Path) -> bool {
        let normalized = self.normalize_path(file_path);
        let locks = self.locks.read().await;
        locks.contains_key(&normalized)
    }

    /// Obtiene quién tiene el bloqueo de un archivo
    pub async fn get_lock_owner(&self, file_path: &Path) -> Option<Uuid> {
        let normalized = self.normalize_path(file_path);
        let locks = self.locks.read().await;
        locks.get(&normalized).map(|l| l.worker_id)
    }

    /// Verifica si un archivo es un hotspot
    pub fn is_hotspot(&self, file_path: &Path) -> bool {
        let path_str = file_path.to_string_lossy();
        self.hotspots.iter().any(|h| {
            path_str == *h
                || path_str.ends_with(&format!("/{}", h))
                || path_str.starts_with(&format!("{}/", h))
        })
    }

    /// Lista todos los bloqueos activos
    pub async fn list_locks(&self) -> Vec<FileLock> {
        self.locks.read().await.values().cloned().collect()
    }

    /// Obtiene estadísticas
    pub async fn get_stats(&self) -> LockStats {
        self.stats.read().await.clone()
    }

    fn normalize_path(&self, path: &Path) -> PathBuf {
        // Normalizar a ruta relativa sin ./ inicial
        let path_str = path.to_string_lossy();
        if let Some(stripped) = path_str.strip_prefix("./") {
            PathBuf::from(stripped)
        } else {
            path.to_path_buf()
        }
    }
}

impl Default for FileLockManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_acquire_and_release() {
        let manager = FileLockManager::new();
        let worker = Uuid::new_v4();
        let file = Path::new("Cargo.toml");

        let result = manager.try_acquire(worker, file).await.unwrap();
        assert_eq!(result, LockResult::Acquired);
        assert!(manager.is_locked(file).await);
        assert_eq!(manager.get_lock_owner(file).await, Some(worker));

        let released = manager.release(worker, file).await;
        assert!(released);
        assert!(!manager.is_locked(file).await);
    }

    #[tokio::test]
    async fn test_conflict_detection() {
        let manager = FileLockManager::new();
        let worker1 = Uuid::new_v4();
        let worker2 = Uuid::new_v4();
        let file = Path::new("Cargo.toml");

        manager.try_acquire(worker1, file).await.unwrap();

        let result = manager.try_acquire(worker2, file).await.unwrap();
        assert!(matches!(result, LockResult::AlreadyLocked { owner } if owner == worker1));

        // Worker1 puede re-adquirir sin problema
        let result = manager.try_acquire(worker1, file).await.unwrap();
        assert_eq!(result, LockResult::Acquired);
    }

    #[tokio::test]
    async fn test_non_hotspot_skips_locking() {
        let manager = FileLockManager::new();
        let worker = Uuid::new_v4();
        let file = Path::new("src/random_file.rs");

        let result = manager.try_acquire(worker, file).await.unwrap();
        assert_eq!(result, LockResult::NotHotspot);
    }

    #[tokio::test]
    async fn test_release_all_for_worker() {
        let manager = FileLockManager::new();
        let worker1 = Uuid::new_v4();
        let worker2 = Uuid::new_v4();

        manager.try_acquire(worker1, Path::new("Cargo.toml")).await.unwrap();
        manager.try_acquire(worker1, Path::new("README.md")).await.unwrap();
        manager.try_acquire(worker2, Path::new("version.json")).await.unwrap();

        let released = manager.release_all(worker1).await;
        assert_eq!(released, 2);

        // Cargo.toml y README.md deben estar libres
        assert!(!manager.is_locked(Path::new("Cargo.toml")).await);
        assert!(!manager.is_locked(Path::new("README.md")).await);
        // version.json sigue bloqueado por worker2
        assert!(manager.is_locked(Path::new("version.json")).await);
    }

    #[tokio::test]
    async fn test_custom_hotspots() {
        let manager = FileLockManager::with_hotspots(vec![
            "custom_config.yaml".to_string(),
            "important.txt".to_string(),
        ]);

        let worker = Uuid::new_v4();

        // Archivo en la lista custom
        let result = manager.try_acquire(worker, Path::new("custom_config.yaml")).await.unwrap();
        assert_eq!(result, LockResult::Acquired);

        // Archivo que NO está en la lista custom
        let result = manager.try_acquire(worker, Path::new("Cargo.toml")).await.unwrap();
        assert_eq!(result, LockResult::NotHotspot);
    }

    #[tokio::test]
    async fn test_stats_tracking() {
        let manager = FileLockManager::new();
        let worker1 = Uuid::new_v4();
        let worker2 = Uuid::new_v4();

        manager.try_acquire(worker1, Path::new("Cargo.toml")).await.unwrap();
        manager.try_acquire(worker2, Path::new("Cargo.toml")).await.unwrap_err_or_conflict();
        manager.release(worker1, Path::new("Cargo.toml")).await;

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_acquired, 1);
        assert_eq!(stats.total_conflicts, 1);
        assert_eq!(stats.total_released, 1);
    }

    #[tokio::test]
    async fn test_timeout_acquisition() {
        let manager = FileLockManager::new();
        let worker1 = Uuid::new_v4();
        let worker2 = Uuid::new_v4();
        let file = Path::new("Cargo.toml");

        manager.try_acquire(worker1, file).await.unwrap();

        // Worker2 intenta con timeout muy corto (debe fallar)
        let acquired = manager.acquire_with_timeout(worker2, file, 0).await.unwrap();
        assert!(!acquired);

        // Worker1 libera
        manager.release(worker1, file).await;

        // Worker2 ahora puede adquirir inmediatamente
        let acquired = manager.acquire_with_timeout(worker2, file, 1).await.unwrap();
        assert!(acquired);
    }

    #[tokio::test]
    async fn test_is_hotspot_detection() {
        let manager = FileLockManager::new();

        assert!(manager.is_hotspot(Path::new("Cargo.toml")));
        assert!(manager.is_hotspot(Path::new("src/../Cargo.toml")));
        assert!(manager.is_hotspot(Path::new("README.md")));
        assert!(!manager.is_hotspot(Path::new("src/main.rs")));
        assert!(!manager.is_hotspot(Path::new("random.txt")));
    }

    #[tokio::test]
    async fn test_list_locks() {
        let manager = FileLockManager::new();
        let worker = Uuid::new_v4();

        manager.try_acquire(worker, Path::new("Cargo.toml")).await.unwrap();
        manager.try_acquire(worker, Path::new("README.md")).await.unwrap();

        let locks = manager.list_locks().await;
        assert_eq!(locks.len(), 2);
    }

    #[tokio::test]
    async fn test_concurrent_workers_different_files() {
        let manager = Arc::new(FileLockManager::new());
        let mut handles = Vec::new();

        for i in 0..5 {
            let mgr = manager.clone();
            let handle = tokio::spawn(async move {
                let worker = Uuid::new_v4();
                let file = PathBuf::from(format!("file_{}.toml", i));

                // Cada worker intenta bloquear un archivo diferente
                // Solo "Cargo.toml" es hotspot, los demás no
                let result = mgr.try_acquire(worker, &file).await.unwrap();
                // file_0.toml, file_1.toml etc no son hotspots
                assert_eq!(result, LockResult::NotHotspot);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }
    }
}

// Helper para el test de stats
#[allow(dead_code)]
trait UnwrapErrOrConflict {
    fn unwrap_err_or_conflict(self);
}

impl UnwrapErrOrConflict for Result<LockResult, anyhow::Error> {
    fn unwrap_err_or_conflict(self) {
        match self {
            Ok(LockResult::AlreadyLocked { .. }) => {}
            Ok(LockResult::Acquired) => panic!("expected conflict"),
            Ok(LockResult::NotHotspot) => panic!("expected conflict"),
            Err(_) => {}
        }
    }
}
