//! Model Context Protocol (MCP) — estandarización de herramientas para agentes.
//!
//! Implementa un cliente MCP que permite a las obreras descubrir y usar herramientas
//! externas de forma estandarizada. Compatible con el protocolo MCP v2 de Anthropic.

#[cfg(feature = "multiagent")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "multiagent")]
use std::collections::HashMap;
#[cfg(feature = "multiagent")]
use tracing::info;

/// Definición de una herramienta MCP.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: Option<serde_json::Value>,
}

/// Parámetros de entrada para invocar una herramienta.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCall {
    pub tool_name: String,
    pub arguments: HashMap<String, serde_json::Value>,
}

/// Resultado de una llamada a herramienta MCP.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    pub success: bool,
    pub content: String,
    pub error: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// Servidor MCP local que expone herramientas del repo.
#[cfg(feature = "multiagent")]
type McpToolHandler = Box<dyn Fn(&McpToolCall) -> McpToolResult + Send + Sync>;

#[cfg(feature = "multiagent")]
pub struct McpServer {
    tools: HashMap<String, McpTool>,
    handlers: HashMap<String, McpToolHandler>,
}

#[cfg(feature = "multiagent")]
impl McpServer {
    pub fn new() -> Self {
        let mut server = Self {
            tools: HashMap::new(),
            handlers: HashMap::new(),
        };
        server.register_default_tools();
        server
    }

    fn register_default_tools(&mut self) {
        // Herramienta: ejecutar comando shell
        self.register_tool(
            McpTool {
                name: "shell_exec".into(),
                description: "Ejecuta un comando shell en el directorio del repo".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {"type": "string", "description": "Comando a ejecutar"},
                        "cwd": {"type": "string", "description": "Directorio de trabajo"}
                    },
                    "required": ["command"]
                }),
                output_schema: None,
            },
            |call| {
                let cmd = call
                    .arguments
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("echo no command");
                let cwd = call
                    .arguments
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");

                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .current_dir(cwd)
                    .output();

                match output {
                    Ok(out) => McpToolResult {
                        success: out.status.success(),
                        content: String::from_utf8_lossy(&out.stdout).to_string(),
                        error: if out.status.success() {
                            None
                        } else {
                            Some(String::from_utf8_lossy(&out.stderr).to_string())
                        },
                        metadata: HashMap::new(),
                    },
                    Err(e) => McpToolResult {
                        success: false,
                        content: String::new(),
                        error: Some(e.to_string()),
                        metadata: HashMap::new(),
                    },
                }
            },
        );

        // Herramienta: leer archivo
        self.register_tool(
            McpTool {
                name: "file_read".into(),
                description: "Lee el contenido de un archivo".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Ruta del archivo"}
                    },
                    "required": ["path"]
                }),
                output_schema: None,
            },
            |call| {
                let path = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match std::fs::read_to_string(path) {
                    Ok(content) => McpToolResult {
                        success: true,
                        content,
                        error: None,
                        metadata: HashMap::new(),
                    },
                    Err(e) => McpToolResult {
                        success: false,
                        content: String::new(),
                        error: Some(e.to_string()),
                        metadata: HashMap::new(),
                    },
                }
            },
        );

        // Herramienta: escribir archivo
        self.register_tool(
            McpTool {
                name: "file_write".into(),
                description: "Escribe contenido a un archivo".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Ruta del archivo"},
                        "content": {"type": "string", "description": "Contenido a escribir"}
                    },
                    "required": ["path", "content"]
                }),
                output_schema: None,
            },
            |call| {
                let path = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let content = call
                    .arguments
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if let Some(parent) = std::path::Path::new(path).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::write(path, content) {
                    Ok(_) => McpToolResult {
                        success: true,
                        content: format!("escrito: {path}"),
                        error: None,
                        metadata: HashMap::new(),
                    },
                    Err(e) => McpToolResult {
                        success: false,
                        content: String::new(),
                        error: Some(e.to_string()),
                        metadata: HashMap::new(),
                    },
                }
            },
        );

        // Herramienta: git diff
        self.register_tool(
            McpTool {
                name: "git_diff".into(),
                description: "Obtiene el diff de cambios actuales".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "repo_root": {"type": "string"},
                        "cached": {"type": "boolean"}
                    },
                    "required": ["repo_root"]
                }),
                output_schema: None,
            },
            |call| {
                let repo = call
                    .arguments
                    .get("repo_root")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");
                let cached = call
                    .arguments
                    .get("cached")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let mut args = vec!["diff"];
                if cached {
                    args.push("--cached");
                }

                let output = std::process::Command::new("git")
                    .args(&args)
                    .current_dir(repo)
                    .output();

                match output {
                    Ok(out) => McpToolResult {
                        success: true,
                        content: String::from_utf8_lossy(&out.stdout).to_string(),
                        error: None,
                        metadata: HashMap::new(),
                    },
                    Err(e) => McpToolResult {
                        success: false,
                        content: String::new(),
                        error: Some(e.to_string()),
                        metadata: HashMap::new(),
                    },
                }
            },
        );

        // Herramienta: git status
        self.register_tool(
            McpTool {
                name: "git_status".into(),
                description: "Obtiene el estado del repositorio Git".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "repo_root": {"type": "string"}
                    },
                    "required": ["repo_root"]
                }),
                output_schema: None,
            },
            |call| {
                let repo = call
                    .arguments
                    .get("repo_root")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");
                let output = std::process::Command::new("git")
                    .args(["status", "--short"])
                    .current_dir(repo)
                    .output();

                match output {
                    Ok(out) => McpToolResult {
                        success: true,
                        content: String::from_utf8_lossy(&out.stdout).to_string(),
                        error: None,
                        metadata: HashMap::new(),
                    },
                    Err(e) => McpToolResult {
                        success: false,
                        content: String::new(),
                        error: Some(e.to_string()),
                        metadata: HashMap::new(),
                    },
                }
            },
        );

        // Herramienta: búsqueda de patrones (grep)
        self.register_tool(
            McpTool {
                name: "code_search".into(),
                description: "Busca un patrón regex en los archivos del repo".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "repo_root": {"type": "string"},
                        "pattern": {"type": "string"},
                        "file_glob": {"type": "string"}
                    },
                    "required": ["repo_root", "pattern"]
                }),
                output_schema: None,
            },
            |call| {
                let repo = call
                    .arguments
                    .get("repo_root")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");
                let pattern = call
                    .arguments
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let glob = call.arguments.get("file_glob").and_then(|v| v.as_str());

                let mut args = vec!["-rn", "--include=*"];
                if let Some(g) = glob {
                    args = vec!["-rn", g];
                }
                args.push(pattern);
                args.push(".");

                let output = std::process::Command::new("grep")
                    .args(&args)
                    .current_dir(repo)
                    .output();

                match output {
                    Ok(out) => McpToolResult {
                        success: true,
                        content: String::from_utf8_lossy(&out.stdout).to_string(),
                        error: None,
                        metadata: HashMap::new(),
                    },
                    Err(e) => McpToolResult {
                        success: false,
                        content: String::new(),
                        error: Some(e.to_string()),
                        metadata: HashMap::new(),
                    },
                }
            },
        );

        info!(count = 6, "herramientas MCP registradas");
    }

    pub fn register_tool(
        &mut self,
        tool: McpTool,
        handler: impl Fn(&McpToolCall) -> McpToolResult + Send + Sync + 'static,
    ) {
        let name = tool.name.clone();
        self.tools.insert(name.clone(), tool);
        self.handlers.insert(name, Box::new(handler));
    }

    /// Lista todas las herramientas disponibles.
    pub fn list_tools(&self) -> Vec<&McpTool> {
        self.tools.values().collect()
    }

    /// Invoca una herramienta por nombre.
    pub fn call_tool(&self, call: &McpToolCall) -> McpToolResult {
        match self.handlers.get(&call.tool_name) {
            Some(handler) => handler(call),
            None => McpToolResult {
                success: false,
                content: String::new(),
                error: Some(format!("herramienta '{}' no encontrada", call.tool_name)),
                metadata: HashMap::new(),
            },
        }
    }

    /// Serializa la lista de herramientas como JSON (para el prompt del LLM).
    pub fn tools_manifest(&self) -> String {
        let tools: Vec<&McpTool> = self.list_tools();
        serde_json::to_string_pretty(&tools).unwrap_or_default()
    }
}

#[cfg(feature = "multiagent")]
impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

// Stubs sin la feature
#[cfg(not(feature = "multiagent"))]
pub struct McpServer;

#[cfg(not(feature = "multiagent"))]
impl McpServer {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "multiagent")]
    #[test]
    fn mcp_server_registra_herramientas() {
        let server = McpServer::new();
        let tools = server.list_tools();
        assert!(tools.len() >= 6);
        assert!(tools.iter().any(|t| t.name == "shell_exec"));
        assert!(tools.iter().any(|t| t.name == "file_read"));
        assert!(tools.iter().any(|t| t.name == "file_write"));
        assert!(tools.iter().any(|t| t.name == "git_diff"));
        assert!(tools.iter().any(|t| t.name == "git_status"));
        assert!(tools.iter().any(|t| t.name == "code_search"));
    }

    #[cfg(feature = "multiagent")]
    #[test]
    fn mcp_server_invoca_file_read() {
        let server = McpServer::new();
        let call = McpToolCall {
            tool_name: "file_read".into(),
            arguments: [(
                "path".into(),
                serde_json::Value::String("/nonexistent_file.txt".into()),
            )]
            .into_iter()
            .collect(),
        };
        let result = server.call_tool(&call);
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[cfg(feature = "multiagent")]
    #[test]
    fn mcp_server_herramienta_desconocida() {
        let server = McpServer::new();
        let call = McpToolCall {
            tool_name: "nonexistent_tool".into(),
            arguments: HashMap::new(),
        };
        let result = server.call_tool(&call);
        assert!(!result.success);
    }

    #[cfg(feature = "multiagent")]
    #[test]
    fn mcp_manifest_es_json_valido() {
        let server = McpServer::new();
        let manifest = server.tools_manifest();
        let parsed: Result<Vec<McpTool>, _> = serde_json::from_str(&manifest);
        assert!(parsed.is_ok());
    }
}
