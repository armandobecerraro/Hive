//! Browser automation para probar apps web.
//!
//! Permite a las obreras verificar UIs, scrappear documentación y probar aplicaciones web.

use serde::{Deserialize, Serialize};

/// Resultado de una operación de browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserResult {
    pub success: bool,
    pub url: String,
    pub status_code: Option<u16>,
    pub content: String,
    pub title: Option<String>,
    pub error: Option<String>,
}

/// Configuración del browser.
#[derive(Debug, Clone)]
pub struct BrowserConfig {
    pub timeout_secs: u64,
    pub user_agent: String,
    pub follow_redirects: bool,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            user_agent: "HiveBrowser/1.0".into(),
            follow_redirects: true,
        }
    }
}

/// Browser headless simplificado.
pub struct HeadlessBrowser {
    _config: BrowserConfig,
}

impl HeadlessBrowser {
    pub fn new(config: BrowserConfig) -> Self {
        Self { _config: config }
    }

    /// Obtiene el contenido de una URL (requiere feature multiagent para HTTP real).
    pub async fn fetch(&self, url: &str) -> BrowserResult {
        #[cfg(feature = "multiagent")]
        {
            let client = match reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(self.config.timeout_secs))
                .user_agent(&self.config.user_agent)
                .build()
            {
                Ok(c) => c,
                Err(e) => {
                    return BrowserResult {
                        success: false,
                        url: url.into(),
                        status_code: None,
                        content: String::new(),
                        title: None,
                        error: Some(e.to_string()),
                    }
                }
            };

            match client.get(url).send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    match resp.text().await {
                        Ok(body) => BrowserResult {
                            success: status < 400,
                            url: url.into(),
                            status_code: Some(status),
                            content: body.clone(),
                            title: extract_title(&body),
                            error: None,
                        },
                        Err(e) => BrowserResult {
                            success: false,
                            url: url.into(),
                            status_code: Some(status),
                            content: String::new(),
                            title: None,
                            error: Some(e.to_string()),
                        },
                    }
                }
                Err(e) => BrowserResult {
                    success: false,
                    url: url.into(),
                    status_code: None,
                    content: String::new(),
                    title: None,
                    error: Some(e.to_string()),
                },
            }
        }
        #[cfg(not(feature = "multiagent"))]
        {
            BrowserResult {
                success: false,
                url: url.into(),
                status_code: None,
                content: String::new(),
                title: None,
                error: Some("feature 'multiagent' requerida para HTTP".into()),
            }
        }
    }

    /// Verifica que una URL esté accesible.
    pub async fn check_url(&self, url: &str) -> bool {
        self.fetch(url).await.success
    }

    /// Extrae texto plano de una URL.
    pub async fn get_text(&self, url: &str) -> Result<String, String> {
        let result = self.fetch(url).await;
        if !result.success {
            return Err(result.error.unwrap_or("fetch failed".into()));
        }
        Ok(strip_html(&result.content))
    }
}

#[allow(dead_code)]
fn extract_title(html: &str) -> Option<String> {
    let start = html.find("<title>")? + 7;
    let end = html[start..].find("</title>")?;
    Some(html[start..start + end].trim().to_string())
}

fn strip_html(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result.split_whitespace().collect::<Vec<&str>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_title_works() {
        let html = "<html><title>Test Page</title><body>Content</body></html>";
        assert_eq!(extract_title(html), Some("Test Page".into()));
    }

    #[test]
    fn strip_html_removes_tags() {
        let html = "<p>Hello <b>world</b></p>";
        assert_eq!(strip_html(html), "Hello world");
    }

    #[test]
    fn browser_config_default() {
        let cfg = BrowserConfig::default();
        assert_eq!(cfg.timeout_secs, 30);
    }
}
