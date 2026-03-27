//! REUSE compliance checker.
//! Verifica que todos los archivos tengan headers de licencia.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ReuseReport {
    pub total_files: usize,
    pub compliant: usize,
    pub non_compliant: Vec<String>,
    pub compliant_percent: f64,
}

pub struct ReuseChecker;

impl ReuseChecker {
    pub fn check_directory(dir: &std::path::Path, extensions: &[&str]) -> ReuseReport {
        let mut total = 0usize;
        let mut compliant = 0usize;
        let mut non_compliant = Vec::new();

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    if name == ".git" || name == "target" || name == "node_modules" {
                        continue;
                    }
                    let sub = Self::check_directory(&path, extensions);
                    total += sub.total_files;
                    compliant += sub.compliant;
                    non_compliant.extend(sub.non_compliant);
                } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if extensions.contains(&ext) {
                        total += 1;
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if Self::has_license_header(&content) {
                                compliant += 1;
                            } else {
                                non_compliant.push(path.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }

        ReuseReport {
            total_files: total,
            compliant,
            non_compliant,
            compliant_percent: if total > 0 {
                compliant as f64 / total as f64 * 100.0
            } else {
                100.0
            },
        }
    }

    fn has_license_header(content: &str) -> bool {
        let first_lines: String = content.lines().take(10).collect::<Vec<&str>>().join("\n");
        first_lines.contains("SPDX-License-Identifier")
            || first_lines.contains("Copyright")
            || first_lines.contains("Licensed under")
            || first_lines.contains("License")
    }

    pub fn generate_header(license: &str, copyright: &str) -> String {
        format!("// SPDX-License-Identifier: {license}\n// Copyright (c) {copyright}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_license_header_detects() {
        assert!(ReuseChecker::has_license_header(
            "// SPDX-License-Identifier: MIT\n// Copyright 2026\n"
        ));
        assert!(!ReuseChecker::has_license_header("fn main() {}\n"));
    }

    #[test]
    fn generate_header_works() {
        let header = ReuseChecker::generate_header("MIT", "2026 Hive");
        assert!(header.contains("MIT"));
        assert!(header.contains("2026 Hive"));
    }
}
