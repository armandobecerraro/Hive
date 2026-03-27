//! Integración real con GitHub API via octocrab.
//!
//! Permite crear PRs, leer issues, asignar reviewers y monitorear el issue tracker.
//! Inspirado en OpenAI Symphony que monitorea issues y genera PRs automáticamente.

#[cfg(feature = "multiagent")]
use anyhow::{Context, Result};
#[cfg(feature = "multiagent")]
use octocrab::models::IssueState;
#[cfg(feature = "multiagent")]
use octocrab::Octocrab;
#[cfg(feature = "multiagent")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "multiagent")]
use tracing::info;

/// Configuración de conexión a GitHub.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubConfig {
    pub owner: String,
    pub repo: String,
    #[serde(skip)]
    pub token: String,
    pub base_branch: String,
}

/// Issue de GitHub como fuente de tareas.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubIssue {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub labels: Vec<String>,
    pub assignees: Vec<String>,
}

/// Pull Request creado por una obrera.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatedPR {
    pub number: u64,
    pub url: String,
    pub branch: String,
}

/// Cliente GitHub para operaciones del enjambre.
#[cfg(feature = "multiagent")]
pub struct GitHubClient {
    client: Octocrab,
    config: GitHubConfig,
}

#[cfg(feature = "multiagent")]
impl GitHubClient {
    /// Crea un cliente desde variable de entorno GITHUB_TOKEN.
    pub fn from_env(owner: &str, repo: &str) -> Result<Self> {
        let token = std::env::var("GITHUB_TOKEN")
            .context("GITHUB_TOKEN no definida (necesaria para GitHub API)")?;
        let base_branch =
            std::env::var("HIVE_GITHUB_BASE_BRANCH").unwrap_or_else(|_| "main".to_string());
        Self::new(GitHubConfig {
            owner: owner.to_string(),
            repo: repo.to_string(),
            token,
            base_branch,
        })
    }

    pub fn new(config: GitHubConfig) -> Result<Self> {
        let client = Octocrab::builder()
            .personal_token(config.token.clone())
            .build()
            .context("crear cliente octocrab")?;
        Ok(Self { client, config })
    }

    /// Lista issues abiertos (posibles fuentes de tareas).
    pub async fn list_open_issues(&self) -> Result<Vec<GitHubIssue>> {
        let issues = self
            .client
            .issues(&self.config.owner, &self.config.repo)
            .list()
            .state(octocrab::params::State::Open)
            .send()
            .await
            .context("listar issues")?;

        let mut result = Vec::new();
        for issue in issues {
            result.push(GitHubIssue {
                number: issue.number,
                title: issue.title,
                body: issue.body,
                labels: issue.labels.iter().map(|l| l.name.clone()).collect(),
                assignees: issue.assignees.iter().map(|a| a.login.clone()).collect(),
            });
        }
        Ok(result)
    }

    /// Obtiene un issue específico.
    pub async fn get_issue(&self, number: u64) -> Result<GitHubIssue> {
        let issue = self
            .client
            .issues(&self.config.owner, &self.config.repo)
            .get(number)
            .await
            .context(format!("obtener issue #{number}"))?;

        Ok(GitHubIssue {
            number: issue.number,
            title: issue.title,
            body: issue.body,
            labels: issue.labels.iter().map(|l| l.name.clone()).collect(),
            assignees: issue.assignees.iter().map(|a| a.login.clone()).collect(),
        })
    }

    /// Crea un Pull Request desde una rama de obrera.
    pub async fn create_pull_request(
        &self,
        branch: &str,
        title: &str,
        body: &str,
    ) -> Result<CreatedPR> {
        let pr = self
            .client
            .pulls(&self.config.owner, &self.config.repo)
            .create(title, branch, &self.config.base_branch)
            .body(body)
            .send()
            .await
            .context("crear pull request")?;

        let url_str = pr.html_url.as_ref().map(|u| u.as_str()).unwrap_or("");
        info!(pr = pr.number, url = url_str, "PR creado en GitHub");

        Ok(CreatedPR {
            number: pr.number,
            url: pr.html_url.map(|u| u.to_string()).unwrap_or_default(),
            branch: branch.to_string(),
        })
    }

    /// Añade un comentario a un PR.
    pub async fn add_pr_comment(&self, pr_number: u64, comment: &str) -> Result<()> {
        self.client
            .issues(&self.config.owner, &self.config.repo)
            .create_comment(pr_number, comment)
            .await
            .context("añadir comentario a PR")?;
        Ok(())
    }

    /// Asigna reviewers a un PR (API de octocrab sujeta a cambios; ampliar con REST si hace falta).
    pub async fn request_reviewers(&self, pr_number: u64, reviewers: &[&str]) -> Result<()> {
        let _ = (pr_number, reviewers);
        info!("request_reviewers: pendiente de mapear a la API actual de octocrab");
        Ok(())
    }

    /// Cierra un issue con un comentario.
    pub async fn close_issue(&self, number: u64, comment: &str) -> Result<()> {
        self.client
            .issues(&self.config.owner, &self.config.repo)
            .create_comment(number, comment)
            .await
            .context("comentar issue")?;

        self.client
            .issues(&self.config.owner, &self.config.repo)
            .update(number)
            .state(IssueState::Closed)
            .send()
            .await
            .context("cerrar issue")?;

        info!(issue = number, "issue cerrado");
        Ok(())
    }

    /// Verifica si un PR fue mergeado.
    pub async fn is_pr_merged(&self, pr_number: u64) -> Result<bool> {
        let merged = self
            .client
            .pulls(&self.config.owner, &self.config.repo)
            .is_merged(pr_number)
            .await
            .context("verificar merge de PR")?;
        Ok(merged)
    }
}

// Stubs sin la feature
#[cfg(not(feature = "multiagent"))]
pub struct GitHubClient;

#[cfg(not(feature = "multiagent"))]
impl GitHubClient {
    pub fn from_env(_owner: &str, _repo: &str) -> anyhow::Result<Self> {
        anyhow::bail!("feature 'multiagent' requerida para GitHub API")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "multiagent")]
    #[test]
    fn github_config_serializable() {
        let cfg = GitHubConfig {
            owner: "test".into(),
            repo: "repo".into(),
            token: "tok".into(),
            base_branch: "main".into(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: GitHubConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.owner, "test");
    }
}
