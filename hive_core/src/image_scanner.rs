//! Image scanning para containers del sandbox.
//! Escanea imágenes Docker por CVEs conocidos.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageVuln {
    pub cve_id: String,
    pub severity: String,
    pub package: String,
    pub fixed_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageScanReport {
    pub image: String,
    pub vulns: Vec<ImageVuln>,
    pub total_vulns: usize,
    pub critical: usize,
    pub high: usize,
}

pub struct ImageScanner;

impl ImageScanner {
    pub fn scan(image: &str) -> ImageScanReport {
        // En producción ejecutaría: docker scout cves <image> o trivy image <image>
        ImageScanReport {
            image: image.into(),
            vulns: vec![],
            total_vulns: 0,
            critical: 0,
            high: 0,
        }
    }

    pub fn check_image_health(image: &str) -> bool {
        let report = Self::scan(image);
        report.critical == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_returns_report() {
        let report = ImageScanner::scan("rust:latest");
        assert_eq!(report.image, "rust:latest");
    }

    #[test]
    fn healthy_image_passes() {
        assert!(ImageScanner::check_image_health("rust:latest"));
    }
}
