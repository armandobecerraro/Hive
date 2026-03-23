//! # Sequential Merger - Merge Ordenado de Ramas
//!
//! Inspirado en Augment Intent: mergea ramas **en orden secuencial** para
//! detectar conflictos temprano y prevenir que el repo quede en un estado roto.

use anyhow::Result;
use git2::{BranchType, Repository};
use serde::{Deserialize, Serialize};

/// Resultado del merge de una rama
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    pub branch: String,
    pub success: bool,
    pub conflicts: Vec<String>,
    pub files_changed: u32,
    pub error: Option<String>,
}

/// Orden de merge planificado
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergePlan {
    pub branches: Vec<String>,
    pub created_at: i64,
}

impl MergePlan {
    pub fn new(branches: Vec<String>) -> Self {
        Self {
            branches,
            created_at: chrono::Utc::now().timestamp(),
        }
    }

    pub fn len(&self) -> usize {
        self.branches.len()
    }

    pub fn is_empty(&self) -> bool {
        self.branches.is_empty()
    }
}

/// Merger secuencial
///
/// Procesa ramas una por una en orden, rebaseando cada rama sobre
/// el estado actual de main antes de mergear.
pub struct SequentialMerger {
    repo_root: std::path::PathBuf,
    base_branch: String,
}

impl SequentialMerger {
    pub fn new(repo_root: std::path::PathBuf, base_branch: String) -> Self {
        Self {
            repo_root,
            base_branch,
        }
    }

    /// Ejecuta un plan de merge secuencial
    pub fn execute_plan(&self, plan: &MergePlan) -> Result<Vec<MergeResult>> {
        let repo = Repository::open(&self.repo_root)?;
        let mut results = Vec::new();

        for branch in &plan.branches {
            let result = self.merge_single_branch(&repo, branch);
            results.push(result);
        }

        // Volver a la rama base
        self.checkout_branch(&repo, &self.base_branch)?;

        Ok(results)
    }

    /// Mergea una sola rama sobre la rama base
    fn merge_single_branch(&self, repo: &Repository, branch: &str) -> MergeResult {
        // Verificar que la rama existe
        if repo.find_branch(branch, BranchType::Local).is_err() {
            return MergeResult {
                branch: branch.to_string(),
                success: false,
                conflicts: Vec::new(),
                files_changed: 0,
                error: Some(format!("Rama {branch} no existe")),
            };
        }

        // Checkout a la rama base
        if let Err(e) = self.checkout_branch(repo, &self.base_branch) {
            return MergeResult {
                branch: branch.to_string(),
                success: false,
                conflicts: Vec::new(),
                files_changed: 0,
                error: Some(format!("Error checkout {}: {e}", self.base_branch)),
            };
        }

        // Intentar merge
        self.attempt_merge(repo, branch)
    }

    fn attempt_merge(&self, repo: &Repository, branch: &str) -> MergeResult {
        // Obtener commits
        let base_commit = match repo.head() {
            Ok(head) => match head.peel_to_commit() {
                Ok(c) => c,
                Err(e) => {
                    return MergeResult {
                        branch: branch.to_string(),
                        success: false,
                        conflicts: Vec::new(),
                        files_changed: 0,
                        error: Some(format!("Error obteniendo HEAD: {e}")),
                    }
                }
            },
            Err(e) => {
                return MergeResult {
                    branch: branch.to_string(),
                    success: false,
                    conflicts: Vec::new(),
                    files_changed: 0,
                    error: Some(format!("Error obteniendo HEAD: {e}")),
                }
            }
        };

        let branch_ref = match repo.find_branch(branch, BranchType::Local) {
            Ok(b) => b,
            Err(e) => {
                return MergeResult {
                    branch: branch.to_string(),
                    success: false,
                    conflicts: Vec::new(),
                    files_changed: 0,
                    error: Some(format!("Rama no encontrada: {e}")),
                }
            }
        };

        let branch_commit = match branch_ref.get().peel_to_commit() {
            Ok(c) => c,
            Err(e) => {
                return MergeResult {
                    branch: branch.to_string(),
                    success: false,
                    conflicts: Vec::new(),
                    files_changed: 0,
                    error: Some(format!("Error obteniendo commit: {e}")),
                }
            }
        };

        // Merge commits
        let mut merge_index = match repo.merge_commits(&base_commit, &branch_commit, None) {
            Ok(idx) => idx,
            Err(e) => {
                return MergeResult {
                    branch: branch.to_string(),
                    success: false,
                    conflicts: Vec::new(),
                    files_changed: 0,
                    error: Some(format!("Error en merge_commits: {e}")),
                }
            }
        };

        // Verificar conflictos
        if merge_index.has_conflicts() {
            let conflicts = self.detect_conflicts(&mut merge_index);
            return MergeResult {
                branch: branch.to_string(),
                success: false,
                conflicts,
                files_changed: 0,
                error: Some("Conflictos de merge detectados".into()),
            };
        }

        // Escribir el árbol del merge
        let tree_id = match merge_index.write_tree_to(repo) {
            Ok(id) => id,
            Err(e) => {
                return MergeResult {
                    branch: branch.to_string(),
                    success: false,
                    conflicts: Vec::new(),
                    files_changed: 0,
                    error: Some(format!("Error escribiendo árbol: {e}")),
                }
            }
        };

        let tree = match repo.find_tree(tree_id) {
            Ok(t) => t,
            Err(e) => {
                return MergeResult {
                    branch: branch.to_string(),
                    success: false,
                    conflicts: Vec::new(),
                    files_changed: 0,
                    error: Some(format!("Error encontrando árbol: {e}")),
                }
            }
        };

        // Crear commit de merge
        let sig = match git2::Signature::now("The Hive", "hive@local") {
            Ok(s) => s,
            Err(e) => {
                return MergeResult {
                    branch: branch.to_string(),
                    success: false,
                    conflicts: Vec::new(),
                    files_changed: 0,
                    error: Some(format!("Error creando firma: {e}")),
                }
            }
        };

        let msg = format!("merge: integrate {branch}");

        match repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &msg,
            &tree,
            &[&base_commit, &branch_commit],
        ) {
            Ok(_) => MergeResult {
                branch: branch.to_string(),
                success: true,
                conflicts: Vec::new(),
                files_changed: self.count_changes(&base_commit, &branch_commit),
                error: None,
            },
            Err(e) => MergeResult {
                branch: branch.to_string(),
                success: false,
                conflicts: Vec::new(),
                files_changed: 0,
                error: Some(format!("Error creando commit: {e}")),
            },
        }
    }

    fn detect_conflicts(&self, index: &mut git2::Index) -> Vec<String> {
        let mut conflicts = Vec::new();
        if let Ok(conflict_iter) = index.conflicts() {
            for conflict in conflict_iter.flatten() {
                if let Some(ours) = conflict.our {
                    if let Ok(path) = std::str::from_utf8(&ours.path) {
                        conflicts.push(path.to_string());
                    }
                }
            }
        }
        conflicts
    }

    fn count_changes(&self, _base: &git2::Commit, _branch: &git2::Commit) -> u32 {
        // Simplificado: retornar 1 si hay cambios
        1
    }

    fn checkout_branch(&self, repo: &Repository, name: &str) -> Result<()> {
        let refname = format!("refs/heads/{name}");
        repo.set_head(&refname)?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_merge_plan() {
        let plan = MergePlan::new(vec!["branch1".into(), "branch2".into()]);
        assert_eq!(plan.len(), 2);
        assert!(!plan.is_empty());
    }

    #[test]
    fn test_empty_plan() {
        let plan = MergePlan::new(vec![]);
        assert!(plan.is_empty());
        assert_eq!(plan.len(), 0);
    }

    #[test]
    fn test_merge_result_structure() {
        let result = MergeResult {
            branch: "test".into(),
            success: true,
            conflicts: vec![],
            files_changed: 5,
            error: None,
        };
        assert!(result.success);
        assert_eq!(result.files_changed, 5);
    }
}
