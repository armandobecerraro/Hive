use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use git2::{Repository, BranchType, Signature, build::CheckoutBuilder};

/// Manages Git operations for the Hive system
pub struct GitManager {
    repo: Option<Repository>,
}

impl std::fmt::Debug for GitManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitManager")
            .field("repo", &self.repo.is_some())
            .finish()
    }
}

impl GitManager {
    /// Creates a new GitManager
    pub fn new() -> Self {
        Self { repo: None }
    }

    /// Initializes or opens a Git repository
    pub fn initialize_repo(&mut self, path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        match Repository::open(path) {
            Ok(repo) => {
                self.repo = Some(repo);
                println!("📁 Opened existing Git repository");
                Ok(())
            }
            Err(_) => {
                // Repository doesn't exist, create one
                let repo = Repository::init(path)?;
                self.repo = Some(repo);
                println!("📁 Initialized new Git repository");
                Ok(())
            }
        }
    }

    /// Creates a new branch
    pub fn create_branch(&self, branch_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let repo = self.repo.as_ref().ok_or("Repository not initialized")?;
        
        // Get the current HEAD commit
        let head = repo.head()?;
        let head_commit = head.peel_to_commit()?;
        
        // Create the branch
        repo.branch(branch_name, &head_commit, false)?;
        
        println!("🌿 Created branch: {}", branch_name);
        Ok(())
    }

    /// Checks out a branch
    pub fn checkout_branch(&self, branch_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let repo = self.repo.as_ref().ok_or("Repository not initialized")?;
        
        // Find the branch
        let branch = repo.find_branch(branch_name, BranchType::Local)?;
        let reference = branch.into_reference();
        
        // Set HEAD to point to this branch
        repo.set_head(reference.name().ok_or("Invalid branch name")?)?;
        
        // Checkout the tree
        let treeish = repo.revparse_single(&format!("refs/heads/{}", branch_name))?;
        let tree = treeish.peel_to_tree()?;
        let mut checkout = CheckoutBuilder::new();
        checkout.force();
        repo.checkout_tree(&tree.as_object(), Some(&mut checkout))?;
        
        println!("✅ Checked out branch: {}", branch_name);
        Ok(())
    }

    /// Commits changes to the repository
    pub fn commit_changes(&self, message: &str, _path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        let repo = self.repo.as_ref().ok_or("Repository not initialized")?;
        
        // Create signature
        let signature = Signature::now("The Hive", "hive@example.com")?;
        
        // Get the current HEAD
        let mut index = repo.index()?;
        
        // Add all changes
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;
        
        // Write the tree
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        
        // Get parent commit
        let parent_commit = match repo.head() {
            Ok(head) => {
                let head_commit = head.peel_to_commit()?;
                vec![head_commit]
            }
            Err(_) => vec![],
        };
        
        // Create commit
        let commit_id = repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_commit.iter().collect::<Vec<_>>(),
        )?;
        
        println!("💾 Committed changes: {} ({})", message, commit_id);
        Ok(())
    }

    /// Registra un MR local (metadatos). El merge en `main` y el borrado de la rama los ejecuta el orquestador (`delete_local_feature_branch_after_integrated_merge`).
    pub fn create_pull_request(&self, branch_name: &str, title: &str, description: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("📤 Creating Pull Request for branch: {}", branch_name);
        println!("   Title: {}", title);
        println!("   Description: {}", description);
        Ok(())
    }

    /// Resuelve `main` o `master` según lo que exista en el repo (Git por defecto).
    fn default_integration_branch(repo: &Repository) -> Result<&'static str, Box<dyn std::error::Error>> {
        if repo.find_branch("main", BranchType::Local).is_ok() {
            Ok("main")
        } else if repo.find_branch("master", BranchType::Local).is_ok() {
            Ok("master")
        } else {
            Err("rama principal main/master no encontrada".into())
        }
    }

    /// Fusiona una rama en la rama principal (`main` o `master`).
    pub fn merge_branch(&self, branch_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let repo = self.repo.as_ref().ok_or("Repository not initialized")?;
        let base = Self::default_integration_branch(repo)?;
        self.checkout_branch(base)?;
        
        // Find the branch to merge
        let branch = repo.find_branch(branch_name, BranchType::Local)?;
        let branch_commit = branch.get().peel_to_commit()?;
        
        // Perform merge
        let mut merge_opts = git2::MergeOptions::new();
        let mut checkout_opts = CheckoutBuilder::new();
        
        // Create annotated commit from branch commit
        let annotated_commit = repo.find_annotated_commit(branch_commit.id())?;
        
        repo.merge(
            &[&annotated_commit],
            Some(&mut merge_opts),
            Some(&mut checkout_opts),
        )?;
        
        // Commit the merge
        let signature = Signature::now("The Hive", "hive@example.com")?;
        let tree = repo.index()?.write_tree()?;
        let tree_obj = repo.find_tree(tree)?;
        
        let head = repo.head()?.peel_to_commit()?;
        
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            &format!("Merge branch '{}' into main", branch_name),
            &tree_obj,
            &[&head, &branch_commit],
        )?;
        
        println!("🔀 Merged branch '{}' into {}", branch_name, base);
        Ok(())
    }

    /// Gets the current branch name
    pub fn get_current_branch(&self) -> Result<String, Box<dyn std::error::Error>> {
        let repo = self.repo.as_ref().ok_or("Repository not initialized")?;
        
        let head = repo.head()?;
        let branch_name = head
            .shorthand()
            .ok_or("Could not get branch name")?
            .to_string();
        
        Ok(branch_name)
    }
}

/// Tras un merge exitoso en `main`, borra la rama de trabajo local. Equivale a cerrar el MR y eliminar la rama en el remoto (aquí solo refs locales vía libgit2).
pub fn delete_local_feature_branch_after_integrated_merge(
    repo_root: &Path,
    branch_name: &str,
) -> Result<bool> {
    let repo = Repository::open(repo_root)
        .with_context(|| format!("abrir repo {}", repo_root.display()))?;
    let deleted = match repo.find_branch(branch_name, BranchType::Local) {
        Ok(mut b) => {
            b.delete()
                .with_context(|| format!("borrar rama local `{branch_name}`"))?;
            true
        }
        Err(_) => false,
    };
    if deleted {
        tracing::info!(
            branch = %branch_name,
            "rama obrera eliminada tras integración en main (MR cerrado a nivel Git local)"
        );
    }
    Ok(deleted)
}

impl Clone for GitManager {
    fn clone(&self) -> Self {
        // Note: Repository doesn't implement Clone, so we create a new one
        // In a real implementation, you'd want to handle this differently
        Self { repo: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_git_manager_creation() {
        let manager = GitManager::new();
        assert!(manager.repo.is_none());
    }

    #[test]
    fn initialize_repo_open_existing() {
        let tmp = TempDir::new().unwrap();
        crate::discovery::colonize_and_analyze(tmp.path()).unwrap();
        let mut gm = GitManager::new();
        gm.initialize_repo(&tmp.path().to_path_buf()).unwrap();
        assert!(gm.repo.is_some());
    }

    #[test]
    fn create_pull_request_simulation_ok() {
        let gm = GitManager::new();
        gm.create_pull_request("feature/x", "título", "cuerpo")
            .unwrap();
    }

    #[test]
    fn clone_clears_repo_handle() {
        let a = GitManager::new();
        let b = a.clone();
        assert!(b.repo.is_none());
    }

    #[test]
    fn branch_and_current_after_initial_commit() {
        let tmp = TempDir::new().unwrap();
        crate::discovery::colonize_and_analyze(tmp.path()).unwrap();
        std::fs::write(tmp.path().join("seed.txt"), "ok").unwrap();
        let mut gm = GitManager::new();
        gm.initialize_repo(&tmp.path().to_path_buf()).unwrap();
        gm.commit_changes("chore: seed", &tmp.path().to_path_buf())
            .unwrap();
        gm.create_branch("hive/test-branch").unwrap();
        gm.checkout_branch("hive/test-branch").unwrap();
        assert_eq!(gm.get_current_branch().unwrap(), "hive/test-branch");
    }

    #[test]
    fn merge_feature_branch_into_default_base() {
        let tmp = TempDir::new().unwrap();
        crate::discovery::colonize_and_analyze(tmp.path()).unwrap();
        std::fs::write(tmp.path().join("base.txt"), "1").unwrap();
        let mut gm = GitManager::new();
        gm.initialize_repo(&tmp.path().to_path_buf()).unwrap();
        gm.commit_changes("init", &tmp.path().to_path_buf()).unwrap();
        gm.create_branch("feature/merge-me").unwrap();
        gm.checkout_branch("feature/merge-me").unwrap();
        std::fs::write(tmp.path().join("only-on-feat.txt"), "x").unwrap();
        gm.commit_changes("feat commit", &tmp.path().to_path_buf())
            .unwrap();
        gm.merge_branch("feature/merge-me").unwrap();
        assert!(tmp.path().join("only-on-feat.txt").exists());
    }

    #[test]
    fn delete_local_branch_after_integrated_merge_removes_ref() {
        let tmp = TempDir::new().unwrap();
        crate::discovery::colonize_and_analyze(tmp.path()).unwrap();
        std::fs::write(tmp.path().join("seed.txt"), "x").unwrap();
        let mut gm = GitManager::new();
        gm.initialize_repo(&tmp.path().to_path_buf()).unwrap();
        gm.commit_changes("init", &tmp.path().to_path_buf()).unwrap();
        let repo = Repository::open(tmp.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("hive/worker/to-delete", &head, false).unwrap();
        assert!(repo
            .find_branch("hive/worker/to-delete", BranchType::Local)
            .is_ok());
        assert!(
            delete_local_feature_branch_after_integrated_merge(tmp.path(), "hive/worker/to-delete")
                .unwrap()
        );
        let repo2 = Repository::open(tmp.path()).unwrap();
        assert!(repo2
            .find_branch("hive/worker/to-delete", BranchType::Local)
            .is_err());
    }
}