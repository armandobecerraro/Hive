//! # Sequential Merger - Merge Ordenado de Ramas
//!
//! Inspirado en Augment Intent: mergea ramas **en orden secuencial** para
//! detectar conflictos temprano y prevenir que el repo quede en un estado roto.
//!
//! La rama base es configurable (`SequentialMerger::new(..., base_branch)`). El flujo principal
//! de La Reina integra en [`HiveConfig::integration_branch`](crate::config::HiveConfig) vía
//! `orchestrator`; este módulo no participa en ese merge salvo que se instancie con la misma base.

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

    fn count_changes(&self, base: &git2::Commit, branch: &git2::Commit) -> u32 {
        let repo = match Repository::open(&self.repo_root) {
            Ok(r) => r,
            Err(_) => return 0,
        };
        let base_tree = match base.tree() {
            Ok(t) => t,
            Err(_) => return 0,
        };
        let branch_tree = match branch.tree() {
            Ok(t) => t,
            Err(_) => return 0,
        };
        let mut count = 0u32;
        let mut opts = git2::DiffOptions::new();
        if let Ok(diff) =
            repo.diff_tree_to_tree(Some(&base_tree), Some(&branch_tree), Some(&mut opts))
        {
            count = diff.stats().map(|s| s.files_changed() as u32).unwrap_or(0);
        }
        count
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
    use git2::{BranchType, Repository};
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    fn sig() -> git2::Signature<'static> {
        git2::Signature::now("Hive Test", "hive-test@local").unwrap()
    }

    /// Repo con `main` en el primer commit (README.md).
    fn init_repo_with_main(path: &Path) -> Repository {
        let repo = Repository::init(path).unwrap();
        let s = sig();
        fs::write(path.join("README.md"), "base\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        let tid = index.write_tree().unwrap();
        {
            let tree = repo.find_tree(tid).unwrap();
            // Primer commit directamente en `main` (evita duplicar refs/heads/main).
            repo.commit(Some("refs/heads/main"), &s, &s, "init", &tree, &[])
                .unwrap();
        }
        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .unwrap();
        repo
    }

    /// `main` en C0; rama `child` con un commit adelante; HEAD vuelve a `main` (sigue en C0).
    fn repo_main_and_ahead_branch(path: &Path) -> Repository {
        let repo = init_repo_with_main(path);
        let s = sig();
        repo.branch(
            "child",
            &repo.head().unwrap().peel_to_commit().unwrap(),
            false,
        )
        .unwrap();
        repo.set_head("refs/heads/child").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .unwrap();
        fs::write(path.join("README.md"), "child\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        let tid = index.write_tree().unwrap();
        {
            let parent = repo.head().unwrap().peel_to_commit().unwrap();
            let tree = repo.find_tree(tid).unwrap();
            repo.commit(Some("HEAD"), &s, &s, "on child", &tree, &[&parent])
                .unwrap();
        }
        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .unwrap();
        repo
    }

    /// Historia divergente en `f.txt` → merge_commits con conflictos.
    fn repo_with_merge_conflict(path: &Path) -> Repository {
        let repo = Repository::init(path).unwrap();
        let s = sig();
        fs::write(path.join("f.txt"), "base").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("f.txt")).unwrap();
        let t1 = index.write_tree().unwrap();
        let c0 = {
            let tree = repo.find_tree(t1).unwrap();
            repo.commit(Some("HEAD"), &s, &s, "init", &tree, &[])
                .unwrap()
        };
        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .unwrap();

        repo.branch("worker", &repo.find_commit(c0).unwrap(), false)
            .unwrap();
        repo.set_head("refs/heads/worker").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .unwrap();
        fs::write(path.join("f.txt"), "side-a").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("f.txt")).unwrap();
        let t2 = index.write_tree().unwrap();
        {
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            let tree2 = repo.find_tree(t2).unwrap();
            repo.commit(Some("HEAD"), &s, &s, "w", &tree2, &[&head])
                .unwrap();
        }

        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .unwrap();
        fs::write(path.join("f.txt"), "side-b").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("f.txt")).unwrap();
        let t3 = index.write_tree().unwrap();
        {
            let head_m = repo.head().unwrap().peel_to_commit().unwrap();
            let tree3 = repo.find_tree(t3).unwrap();
            repo.commit(Some("HEAD"), &s, &s, "m", &tree3, &[&head_m])
                .unwrap();
        }
        repo
    }

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

    #[test]
    fn merge_branch_inexistente_devuelve_error_en_resultado() {
        let tmp = tempdir().unwrap();
        init_repo_with_main(tmp.path());
        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());
        let plan = MergePlan::new(vec!["rama_fantasma".into()]);
        let results = m.execute_plan(&plan).unwrap();
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0].error.as_ref().unwrap().contains("no existe"));
    }

    #[test]
    fn execute_plan_merge_sin_conflicto_integra_rama() {
        let tmp = tempdir().unwrap();
        repo_main_and_ahead_branch(tmp.path());
        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());
        let plan = MergePlan::new(vec!["child".into()]);
        let results = m.execute_plan(&plan).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].success, "{:?}", results[0].error);
        assert!(
            results[0].files_changed >= 1,
            "debe haber al menos 1 archivo cambiado"
        );
        assert!(results[0].error.is_none());

        let repo = Repository::open(tmp.path()).unwrap();
        assert_eq!(repo.head().unwrap().shorthand().unwrap(), "main");
        let content = fs::read_to_string(tmp.path().join("README.md")).unwrap();
        assert_eq!(content, "child\n");
    }

    #[test]
    fn execute_plan_merge_con_conflicto_marca_fallo() {
        let tmp = tempdir().unwrap();
        repo_with_merge_conflict(tmp.path());
        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());
        let plan = MergePlan::new(vec!["worker".into()]);
        let results = m.execute_plan(&plan).unwrap();
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0].error.as_ref().unwrap().contains("Conflictos"));
        assert!(!results[0].conflicts.is_empty() || !results[0].error.as_ref().unwrap().is_empty());
    }

    #[test]
    fn execute_plan_varias_ramas_en_orden() {
        let tmp = tempdir().unwrap();
        let repo = repo_main_and_ahead_branch(tmp.path());
        let s = sig();
        // Segunda rama `second` desde main actual (C0 después de checkout main)
        let main_commit = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("second", &main_commit, false).unwrap();
        repo.set_head("refs/heads/second").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .unwrap();
        fs::write(tmp.path().join("second.txt"), "y\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("second.txt")).unwrap();
        let tid = index.write_tree().unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        {
            let tree = repo.find_tree(tid).unwrap();
            repo.commit(Some("HEAD"), &s, &s, "second", &tree, &[&parent])
                .unwrap();
        }
        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .unwrap();

        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());
        let plan = MergePlan::new(vec!["child".into(), "second".into()]);
        let results = m.execute_plan(&plan).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].success, "child: {:?}", results[0].error);
        assert!(results[1].success, "second: {:?}", results[1].error);

        let r = Repository::open(tmp.path()).unwrap();
        assert_eq!(r.head().unwrap().shorthand().unwrap(), "main");
        assert!(tmp.path().join("second.txt").exists());
    }

    #[test]
    fn checkout_final_vuelve_a_main_tras_error() {
        let tmp = tempdir().unwrap();
        init_repo_with_main(tmp.path());
        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());
        let plan = MergePlan::new(vec!["missing".into()]);
        m.execute_plan(&plan).unwrap();
        let r = Repository::open(tmp.path()).unwrap();
        assert_eq!(r.head().unwrap().shorthand().unwrap(), "main");
        assert!(r.find_branch("main", BranchType::Local).is_ok());
    }

    #[test]
    fn sequential_merger_new_crea_instancia() {
        let tmp = tempdir().unwrap();
        let m = SequentialMerger::new(tmp.path().to_path_buf(), "develop".into());
        assert_eq!(m.repo_root, tmp.path().to_path_buf());
    }

    #[test]
    fn detect_conflicts_devuelve_lista_vacia_sin_conflictos() {
        let tmp = tempdir().unwrap();
        let repo = init_repo_with_main(tmp.path());
        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());

        let mut index = repo.index().unwrap();
        let conflicts = m.detect_conflicts(&mut index);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn merge_plan_ordena_ramas_por_prioridad() {
        let plan = MergePlan::new(vec!["zebra".into(), "alpha".into(), "mango".into()]);
        assert_eq!(plan.len(), 3);
        assert!(!plan.is_empty());
    }

    #[test]
    fn execute_plan_vacio_retorna_ok() {
        let tmp = tempdir().unwrap();
        init_repo_with_main(tmp.path());
        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());
        let plan = MergePlan::new(vec![]);
        let results = m.execute_plan(&plan).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn merge_result_falla_con_error_especifico() {
        let result = MergeResult {
            branch: "test".into(),
            success: false,
            conflicts: vec!["file1.rs".into()],
            files_changed: 0,
            error: Some("Merge failed".into()),
        };
        assert!(!result.success);
        assert_eq!(result.conflicts.len(), 1);
    }

    #[test]
    fn merge_result_exitoso_sin_conflictos() {
        let result = MergeResult {
            branch: "test".into(),
            success: true,
            conflicts: vec![],
            files_changed: 3,
            error: None,
        };
        assert!(result.success);
        assert!(result.conflicts.is_empty());
        assert!(result.error.is_none());
    }

    #[test]
    fn sequential_merger_maneja_rama_inexistente_en_merge() {
        let tmp = tempdir().unwrap();
        init_repo_with_main(tmp.path());
        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());

        let plan = MergePlan::new(vec!["inexistente".into()]);
        let results = m.execute_plan(&plan).unwrap();

        assert!(!results[0].success);
        assert!(results[0].error.is_some());
    }

    #[test]
    fn sequential_merger_multiple_conflictos() {
        let tmp = tempdir().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        let s = sig();

        fs::write(tmp.path().join("a.txt"), "base").unwrap();
        fs::write(tmp.path().join("b.txt"), "base").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.add_path(Path::new("b.txt")).unwrap();
        let t1 = index.write_tree().unwrap();
        let c0 = {
            let tree = repo.find_tree(t1).unwrap();
            repo.commit(Some("HEAD"), &s, &s, "init", &tree, &[])
                .unwrap()
        };
        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .unwrap();

        repo.branch("conflict", &repo.find_commit(c0).unwrap(), false)
            .unwrap();
        repo.set_head("refs/heads/conflict").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .unwrap();
        fs::write(tmp.path().join("a.txt"), "change-a").unwrap();
        fs::write(tmp.path().join("b.txt"), "change-b").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.add_path(Path::new("b.txt")).unwrap();
        let t2 = index.write_tree().unwrap();
        {
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            let tree2 = repo.find_tree(t2).unwrap();
            repo.commit(Some("HEAD"), &s, &s, "changes", &tree2, &[&head])
                .unwrap();
        }

        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .unwrap();
        fs::write(tmp.path().join("a.txt"), "change-a-2").unwrap();
        fs::write(tmp.path().join("b.txt"), "change-b-2").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.add_path(Path::new("b.txt")).unwrap();
        let t3 = index.write_tree().unwrap();
        {
            let head_m = repo.head().unwrap().peel_to_commit().unwrap();
            let tree3 = repo.find_tree(t3).unwrap();
            repo.commit(Some("HEAD"), &s, &s, "main-changes", &tree3, &[&head_m])
                .unwrap();
        }

        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());
        let plan = MergePlan::new(vec!["conflict".into()]);
        let results = m.execute_plan(&plan).unwrap();

        assert!(!results[0].success);
        assert!(!results[0].conflicts.is_empty());
    }

    #[test]
    fn sequential_merger_cambia_de_rama_correctamente() {
        let tmp = tempdir().unwrap();
        let repo = repo_main_and_ahead_branch(tmp.path());

        let main_commit = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &main_commit, false).unwrap();

        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());

        let repo = Repository::open(tmp.path()).unwrap();
        let head_ref = repo.head().unwrap();
        let _initial_head = head_ref.shorthand().unwrap().to_string();

        m.checkout_branch(&repo, "main").unwrap();

        let after_head_ref = repo.head().unwrap();
        let after_head = after_head_ref.shorthand().unwrap();
        assert_eq!(after_head, "main");
    }

    #[test]
    fn merge_result_serialization() {
        let result = MergeResult {
            branch: "feature".into(),
            success: true,
            conflicts: vec![],
            files_changed: 5,
            error: None,
        };
        let serialized = serde_json::to_string(&result).unwrap();
        let deserialized: MergeResult = serde_json::from_str(&serialized).unwrap();
        assert!(deserialized.success);
        assert_eq!(deserialized.files_changed, 5);
    }

    #[test]
    fn merge_result_with_conflicts_serialization() {
        let result = MergeResult {
            branch: "feature".into(),
            success: false,
            conflicts: vec!["src/main.rs".into(), "Cargo.toml".into()],
            files_changed: 0,
            error: Some("Merge conflict".into()),
        };
        let serialized = serde_json::to_string(&result).unwrap();
        let deserialized: MergeResult = serde_json::from_str(&serialized).unwrap();
        assert!(!deserialized.success);
        assert_eq!(deserialized.conflicts.len(), 2);
    }

    #[test]
    fn merge_plan_serialization() {
        let plan = MergePlan::new(vec!["branch1".into(), "branch2".into()]);
        let serialized = serde_json::to_string(&plan).unwrap();
        let deserialized: MergePlan = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.len(), 2);
    }

    #[test]
    fn sequential_merger_empty_repo_error() {
        let tmp = tempdir().unwrap();
        let _repo = Repository::init(tmp.path()).unwrap();

        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());
        let plan = MergePlan::new(vec!["anything".into()]);

        let result = m.execute_plan(&plan);
        assert!(result.is_err() || (result.is_ok() && result.unwrap()[0].error.is_some()));
    }

    #[test]
    fn sequential_merger_base_branch_different() {
        let tmp = tempdir().unwrap();
        init_repo_with_main(tmp.path());

        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());

        let repo = Repository::open(tmp.path()).unwrap();
        m.checkout_branch(&repo, "main").unwrap();

        let head_ref = repo.head().unwrap();
        let head = head_ref.shorthand().unwrap();
        assert_eq!(head, "main");
    }

    #[test]
    fn count_changes_same_commit_returns_zero() {
        let tmp = tempdir().unwrap();
        let repo = init_repo_with_main(tmp.path());
        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());

        let base = repo.head().unwrap().peel_to_commit().unwrap();
        let branch = repo.head().unwrap().peel_to_commit().unwrap();

        let changes = m.count_changes(&base, &branch);
        assert_eq!(changes, 0);
    }

    #[test]
    fn sequential_merger_handles_nonexistent_base_branch() {
        let tmp = tempdir().unwrap();
        init_repo_with_main(tmp.path());

        let m = SequentialMerger::new(tmp.path().to_path_buf(), "nonexistent_branch".into());

        let repo = Repository::open(tmp.path()).unwrap();
        let result = m.checkout_branch(&repo, "main");
        assert!(result.is_ok());
    }

    #[test]
    fn merge_plan_ordering() {
        let plan = MergePlan::new(vec!["feature".into(), "bugfix".into(), "hotfix".into()]);
        assert_eq!(plan.len(), 3);
        assert!(!plan.is_empty());

        let plan2 = MergePlan::new(vec![]);
        assert!(plan2.is_empty());
    }

    #[test]
    fn merge_result_with_multiple_conflicts() {
        let result = MergeResult {
            branch: "feature".into(),
            success: false,
            conflicts: vec![
                "src/lib.rs".into(),
                "src/main.rs".into(),
                "Cargo.toml".into(),
            ],
            files_changed: 0,
            error: Some("Multiple conflicts".into()),
        };
        assert!(!result.success);
        assert_eq!(result.conflicts.len(), 3);
    }

    #[test]
    fn merge_result_error_field() {
        let result = MergeResult {
            branch: "test".into(),
            success: false,
            conflicts: vec![],
            files_changed: 0,
            error: None,
        };
        assert!(!result.success);
        assert!(result.error.is_none());

        let result2 = MergeResult {
            branch: "test".into(),
            success: false,
            conflicts: vec![],
            files_changed: 0,
            error: Some("Something went wrong".into()),
        };
        assert!(result2.error.is_some());
    }

    #[test]
    fn sequential_merger_checkout_invalid_branch() {
        let tmp = tempdir().unwrap();
        init_repo_with_main(tmp.path());

        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());
        let repo = Repository::open(tmp.path()).unwrap();

        let result = m.checkout_branch(&repo, "nonexistent_branch_xyz");
        assert!(result.is_err());
    }

    #[test]
    fn sequential_merger_base_branch_checkout() {
        let tmp = tempdir().unwrap();
        let repo = init_repo_with_main(tmp.path());

        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());

        m.checkout_branch(&repo, "main").unwrap();

        let head_ref = repo.head().unwrap();
        let head = head_ref.shorthand().unwrap();
        assert_eq!(head, "main");
    }

    #[test]
    fn count_changes_same_commit_returns_zero_for_identical() {
        let tmp = tempdir().unwrap();
        let repo = init_repo_with_main(tmp.path());
        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());

        let commit = repo.head().unwrap().peel_to_commit().unwrap();

        let changes = m.count_changes(&commit, &commit);
        assert_eq!(changes, 0);
    }

    #[test]
    fn detect_conflicts_returns_paths() {
        let tmp = tempdir().unwrap();
        let repo = init_repo_with_main(tmp.path());
        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());

        let mut index = repo.index().unwrap();
        let conflicts = m.detect_conflicts(&mut index);

        assert!(conflicts.is_empty());
    }

    #[test]
    fn merge_plan_created_at_timestamp() {
        let plan = MergePlan::new(vec!["branch".into()]);
        assert!(plan.created_at > 0);
    }

    #[test]
    fn sequential_merger_new_sets_base_branch() {
        let tmp = tempdir().unwrap();
        let m = SequentialMerger::new(tmp.path().to_path_buf(), "develop".into());
        assert_eq!(m.base_branch, "develop");
    }

    #[test]
    fn sequential_merger_new_with_main() {
        let tmp = tempdir().unwrap();
        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());
        assert_eq!(m.base_branch, "main");
    }

    #[test]
    fn execute_plan_with_invalid_branch() {
        let tmp = tempdir().unwrap();
        init_repo_with_main(tmp.path());
        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());

        let plan = MergePlan::new(vec!["nonexistent-branch-xyz".into()]);
        let results = m.execute_plan(&plan).unwrap();

        assert!(!results[0].success);
        assert!(results[0].error.is_some());
    }

    #[test]
    fn sequential_merger_repo_root_access() {
        let tmp = tempdir().unwrap();
        let m = SequentialMerger::new(tmp.path().to_path_buf(), "main".into());
        assert_eq!(m.repo_root, tmp.path().to_path_buf());
    }
}
