//! Análisis de seguridad de la supply chain.
//! Escanea dependencias por vulnerabilidades conocidas.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vulnerability {
    pub package: String,
    pub version: String,
    pub severity: VulnSeverity,
    pub description: String,
    pub advisory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VulnSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupplyChainReport {
    pub total_deps: usize,
    pub vulns: Vec<Vulnerability>,
    pub clean: bool,
}

pub struct SupplyChainScanner;

impl SupplyChainScanner {
    /// Escanea un Cargo.lock (simulado).
    pub fn scan_cargo_lock(content: &str) -> SupplyChainReport {
        let dep_count = content
            .lines()
            .filter(|l| l.trim_start().starts_with("name ="))
            .count();
        SupplyChainReport {
            total_deps: dep_count,
            vulns: vec![],
            clean: true,
        }
    }

    /// Escanea un package-lock.json (simulado).
    pub fn scan_npm_lock(content: &str) -> SupplyChainReport {
        let clean = !content.contains("vulnerability");
        SupplyChainReport {
            total_deps: 0,
            vulns: vec![],
            clean,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_cargo_lock_counts_deps() {
        let lock = "name = \"serde\"\nversion = \"1.0\"\nname = \"tokio\"\nversion = \"1.0\"\n";
        let report = SupplyChainScanner::scan_cargo_lock(lock);
        assert_eq!(report.total_deps, 2);
        assert!(report.clean);
    }
}
