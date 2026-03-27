//! Análisis de dependencias entre módulos.
//!
//! Detecta imports, llamadas, dependencias circulares.
//! Inspirado en tree-sitter y rust-analyzer.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Dependencia entre archivos.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub from: String,
    pub to: String,
    pub dep_type: DependencyType,
}

/// Tipo de dependencia.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DependencyType {
    Import,
    Module,
    FunctionCall,
    TypeUsage,
}

/// Grafo de dependencias.
pub struct DependencyGraph {
    edges: Vec<Dependency>,
    adjacency: HashMap<String, HashSet<String>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            edges: Vec::new(),
            adjacency: HashMap::new(),
        }
    }

    /// Analiza imports de un archivo Rust.
    pub fn analyze_rust_imports(&mut self, file: &str, content: &str) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("use ") && line.ends_with(';') {
                let module = line[4..line.len() - 1].trim().to_string();
                self.add_dependency(file, &module, DependencyType::Import);
            } else if line.starts_with("mod ") && line.ends_with(';') {
                let module = line[4..line.len() - 1].trim().to_string();
                self.add_dependency(file, &module, DependencyType::Module);
            }
        }
    }

    /// Analiza imports de un archivo Python.
    pub fn analyze_python_imports(&mut self, file: &str, content: &str) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("import ") {
                let module = line[7..]
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string();
                if !module.is_empty() {
                    self.add_dependency(file, &module, DependencyType::Import);
                }
            } else if line.starts_with("from ") {
                if let Some(module) = line[5..].split_whitespace().next() {
                    self.add_dependency(file, module, DependencyType::Import);
                }
            }
        }
    }

    /// Analiza imports de un archivo JavaScript/TypeScript.
    pub fn analyze_js_imports(&mut self, file: &str, content: &str) {
        for line in content.lines() {
            let line = line.trim();
            if (line.starts_with("import ") || line.starts_with("const "))
                && line.contains("require(")
            {
                if let Some(start) = line.find("require('") {
                    let rest = &line[start + 9..];
                    if let Some(end) = rest.find('\'') {
                        self.add_dependency(file, &rest[..end], DependencyType::Import);
                    }
                }
            } else if line.starts_with("import ") && line.contains(" from '") {
                if let Some(start) = line.rfind(" from '") {
                    let rest = &line[start + 7..];
                    if let Some(end) = rest.find('\'') {
                        self.add_dependency(file, &rest[..end], DependencyType::Import);
                    }
                }
            }
        }
    }

    fn add_dependency(&mut self, from: &str, to: &str, dep_type: DependencyType) {
        self.edges.push(Dependency {
            from: from.into(),
            to: to.into(),
            dep_type,
        });
        self.adjacency
            .entry(from.into())
            .or_default()
            .insert(to.into());
    }

    /// Detecta ciclos en el grafo.
    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut path = Vec::new();

        for node in self.adjacency.keys() {
            if !visited.contains(node) {
                self.dfs_cycles(node, &mut visited, &mut path, &mut cycles);
            }
        }
        cycles
    }

    fn dfs_cycles(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        if let Some(pos) = path.iter().position(|n| n == node) {
            cycles.push(path[pos..].to_vec());
            return;
        }
        if visited.contains(node) {
            return;
        }

        visited.insert(node.into());
        path.push(node.into());

        if let Some(neighbors) = self.adjacency.get(node) {
            for neighbor in neighbors {
                self.dfs_cycles(neighbor, visited, path, cycles);
            }
        }

        path.pop();
    }

    /// Lista todas las dependencias.
    pub fn edges(&self) -> &[Dependency] {
        &self.edges
    }

    /// Cuenta dependencias.
    pub fn len(&self) -> usize {
        self.edges.len()
    }
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_rust_imports() {
        let mut graph = DependencyGraph::new();
        graph.analyze_rust_imports("main.rs", "use std::collections::HashMap;\nmod lib;");
        assert_eq!(graph.len(), 2);
    }

    #[test]
    fn analyze_python_imports() {
        let mut graph = DependencyGraph::new();
        graph.analyze_python_imports("main.py", "import os\nfrom sys import argv");
        assert_eq!(graph.len(), 2);
    }

    #[test]
    fn detect_cycles() {
        let mut graph = DependencyGraph::new();
        graph.add_dependency("a", "b", DependencyType::Import);
        graph.add_dependency("b", "c", DependencyType::Import);
        graph.add_dependency("c", "a", DependencyType::Import);
        let cycles = graph.detect_cycles();
        assert!(!cycles.is_empty());
    }
}
