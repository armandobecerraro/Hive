//! Plugins WASM — registro de especialistas.
//!
//! La API está preparada para integrar un motor WASM (p. ej. Wasmer) cuando el
//! toolchain y las versiones se alineen; por ahora el registro solo indexa
//! rutas y metadatos mínimos sin ejecutar binarios.

#[cfg(feature = "multiagent")]
use anyhow::{Context, Result};
#[cfg(feature = "multiagent")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "multiagent")]
use std::path::{Path, PathBuf};
#[cfg(feature = "multiagent")]
use tracing::info;

/// Metadatos de un plugin WASM.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub language_key: String,
    pub capabilities: Vec<String>,
    pub wasm_path: PathBuf,
}

/// Resultado de ejecución de un plugin.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginResult {
    pub plugin_name: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

/// Registro de plugins (metadatos; ejecución WASM pendiente de motor embebido).
#[cfg(feature = "multiagent")]
pub struct WasmPluginRegistry {
    plugins: Vec<PluginMetadata>,
}

#[cfg(feature = "multiagent")]
impl Default for WasmPluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "multiagent")]
impl WasmPluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Registra un `.wasm` por ruta (no se valida ni ejecuta el binario).
    pub fn load_plugin(&mut self, wasm_path: &Path) -> Result<&PluginMetadata> {
        let wasm_bytes = std::fs::read(wasm_path)
            .with_context(|| format!("leer plugin WASM: {}", wasm_path.display()))?;
        if wasm_bytes.is_empty() {
            anyhow::bail!("archivo WASM vacío");
        }

        let name = wasm_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let metadata = PluginMetadata {
            name: name.clone(),
            version: "0.1.0".into(),
            author: "wasm-plugin".into(),
            description: format!("Plugin WASM: {name}"),
            language_key: "wasm".into(),
            capabilities: vec!["execute".into()],
            wasm_path: wasm_path.to_path_buf(),
        };

        info!(name = %name, path = %wasm_path.display(), "plugin WASM registrado");
        self.plugins.push(metadata);
        Ok(self.plugins.last().unwrap())
    }

    /// Carga todos los plugins de un directorio.
    pub fn load_plugins_from_dir(&mut self, dir: &Path) -> Result<usize> {
        let mut count = 0;
        if !dir.exists() {
            return Ok(0);
        }
        for entry in std::fs::read_dir(dir).context("leer directorio de plugins")? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("wasm")
                && self.load_plugin(&path).is_ok()
            {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Lista los plugins cargados.
    pub fn list_plugins(&self) -> &[PluginMetadata] {
        &self.plugins
    }

    /// Busca plugins por language_key.
    pub fn find_by_language(&self, language: &str) -> Vec<&PluginMetadata> {
        self.plugins
            .iter()
            .filter(|p| p.language_key == language)
            .collect()
    }

    /// Ejecución en runtime: pendiente de integración con motor WASM.
    pub fn execute_plugin(&mut self, name: &str, _input: &str) -> Result<PluginResult> {
        let _meta = self
            .plugins
            .iter()
            .find(|p| p.name == name)
            .context(format!("plugin '{name}' no encontrado"))?;
        anyhow::bail!(
            "ejecución WASM no disponible (motor embebido pendiente); plugin '{name}'"
        );
    }
}

#[cfg(not(feature = "multiagent"))]
pub struct WasmPluginRegistry;

#[cfg(not(feature = "multiagent"))]
impl WasmPluginRegistry {
    pub fn new() -> Self {
        Self
    }
    pub fn list_plugins(&self) -> &[()] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "multiagent")]
    #[test]
    fn wasm_registry_vacia() {
        let registry = WasmPluginRegistry::new();
        assert!(registry.list_plugins().is_empty());
    }

    #[cfg(feature = "multiagent")]
    #[test]
    fn wasm_registry_carga_dir_inexistente() {
        let mut registry = WasmPluginRegistry::new();
        let count = registry
            .load_plugins_from_dir(Path::new("/nonexistent_plugins"))
            .unwrap();
        assert_eq!(count, 0);
    }
}
