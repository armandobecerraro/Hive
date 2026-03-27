//! Modelo de amenazas y postura de seguridad de Hive.
//!
//! Este módulo documenta QUÉ datos salen del repo, QUÉ amenazas existen,
//! y CÓMO el sistema las mitiga. Es lo que piden los equipos de seguridad
//! antes de adoptar cualquier herramienta que procese código.
//!
//! Alineado con: OWASP, NIST, ISO 27001, SOC2.

use serde::{Deserialize, Serialize};

/// Categoría de amenaza (OWASP-aligned).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ThreatCategory {
    Injection,            // Command/SQL/Code injection
    DataExfiltration,     // Datos saliendo del repo
    AuthenticationBypass, // Tokens/API keys comprometidos
    PrivilegeEscalation,  // Acceso no autorizado a ramas/acciones
    DenialOfService,      // Recursos agotados
    SupplyChain,          // Dependencias comprometidas
    InsufficientLogging,  // Falta de auditoría
    InsecureConfig,       // Configuración débil
}

/// Severidad de amenaza.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ThreatSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Una amenaza identificada.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Threat {
    pub id: String,
    pub category: ThreatCategory,
    pub severity: ThreatSeverity,
    pub description: String,
    pub attack_vector: String,
    pub impact: String,
    pub mitigation: String,
    pub residual_risk: String,
}

/// Dato que sale del repositorio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFlow {
    pub data_type: String,
    pub destination: String,
    pub encryption: bool,
    pub consent_required: bool,
    pub retention_days: usize,
}

/// Postura de seguridad completa.
pub struct SecurityPosture;

impl SecurityPosture {
    /// Devuelve el modelo de amenazas completo.
    pub fn threat_model() -> Vec<Threat> {
        vec![
            Threat {
                id: "THR-001".into(),
                category: ThreatCategory::Injection,
                severity: ThreatSeverity::Critical,
                description: "Inyección de comandos a través de herramientas MCP o prompts del LLM".into(),
                attack_vector: "Un prompt adversarial genera un `shell_exec` con `rm -rf /` o similar".into(),
                impact: "Ejecución arbitraria de comandos en el host".into(),
                mitigation: "Allowlist de comandos MCP. Validación de paths contra traversal. Sandbox Docker para obreras.".into(),
                residual_risk: "Bajo si se mantiene la allowlist actualizada".into(),
            },
            Threat {
                id: "THR-002".into(),
                category: ThreatCategory::DataExfiltration,
                severity: ThreatSeverity::High,
                description: "El LLM puede generar código que envía datos del repo a endpoints externos".into(),
                attack_vector: "El LLM genera un script que hace curl a un servidor externo con contenido del repo".into(),
                impact: "Filtración de código fuente, secrets, datos propietarios".into(),
                mitigation: "Network policies en sandbox (modo 'none' por defecto). Auditoría de conexiones de red.".into(),
                residual_risk: "Medio: depende de la política de red configurada".into(),
            },
            Threat {
                id: "THR-003".into(),
                category: ThreatCategory::AuthenticationBypass,
                severity: ThreatSeverity::High,
                description: "Tokens de API (GitHub, OpenAI) expuestos en logs o persistencia".into(),
                attack_vector: "GitHubConfig derivaba Serialize exponiendo el token. Logs de tracing pueden incluir configs.".into(),
                impact: "Compromiso de cuentas de GitHub/OpenAI del usuario".into(),
                mitigation: "SecretString redacta en Debug/Display/Serialize. Token no se persiste en hive.json.".into(),
                residual_risk: "Bajo si se usa SecretString consistentemente".into(),
            },
            Threat {
                id: "THR-004".into(),
                category: ThreatCategory::PrivilegeEscalation,
                severity: ThreatSeverity::Medium,
                description: "Una obrera puede modificar ramas que no debería".into(),
                attack_vector: "Sin HIVE_PROTECT_MAIN, una obrera puede commitear directamente a main".into(),
                impact: "Corrupción de la rama principal sin revisión".into(),
                mitigation: "HIVE_PROTECT_MAIN=1 por defecto en producción. Ramas huérfanas aisladas.".into(),
                residual_risk: "Bajo con la política de ramas activada".into(),
            },
            Threat {
                id: "THR-005".into(),
                category: ThreatCategory::SupplyChain,
                severity: ThreatSeverity::Medium,
                description: "Dependencias comprometidas en Cargo.toml o npm".into(),
                attack_vector: "Una dependencia maliciosa ejecuta código al compilarse o importarse".into(),
                impact: "Ejecución de código arbitrario durante el build".into(),
                mitigation: "Supply chain scanner. SBOM en CI. Cargo audit automatizado.".into(),
                residual_risk: "Medio: depende de la frecuencia de escaneo".into(),
            },
            Threat {
                id: "THR-006".into(),
                category: ThreatCategory::DenialOfService,
                severity: ThreatSeverity::Medium,
                description: "Un prompt adversarial genera un loop infinito o consumo excesivo de tokens".into(),
                attack_vector: "LLM genera código con loop infinito. Tokens ilimitados sin rate limiting.".into(),
                impact: "Agotamiento de cuota API o recursos del host".into(),
                mitigation: "Rate limiting por proveedor. Timeout por obrera. Cost tracking con alertas.".into(),
                residual_risk: "Bajo con rate limiting activo".into(),
            },
            Threat {
                id: "THR-007".into(),
                category: ThreatCategory::InsufficientLogging,
                severity: ThreatSeverity::Medium,
                description: "Falta de auditoría de quién ejecutó qué acción".into(),
                attack_vector: "Un usuario ejecuta un ciclo y no queda registro de quién fue".into(),
                impact: "Imposibilidad de investigación forense ante un incidente".into(),
                mitigation: "Audit log JSONL con actor, acción, timestamp. Pipeline reports por ciclo.".into(),
                residual_risk: "Bajo con audit log habilitado".into(),
            },
            Threat {
                id: "THR-008".into(),
                category: ThreatCategory::InsecureConfig,
                severity: ThreatSeverity::Low,
                description: "Configuración por defecto insegura".into(),
                attack_vector: "HIVE_PROTECT_MAIN=false por defecto permite escritura en main".into(),
                impact: "Accidental merge sin revisión".into(),
                mitigation: "Documentación clara. Validación de config al inicio. Defaults seguros en producción.".into(),
                residual_risk: "Bajo si se siguen las guías".into(),
            },
        ]
    }

    /// Devuelve el mapa de datos que salen del repositorio.
    pub fn data_flows() -> Vec<DataFlow> {
        vec![
            DataFlow {
                data_type: "Código fuente".into(),
                destination: "API del LLM (OpenAI, Anthropic, Ollama local)".into(),
                encryption: true,
                consent_required: true,
                retention_days: 0,
            },
            DataFlow {
                data_type: "Prompts y respuestas".into(),
                destination: "Logs locales (.hive/pipeline_reports/)".into(),
                encryption: false,
                consent_required: false,
                retention_days: 90,
            },
            DataFlow {
                data_type: "Métricas de uso".into(),
                destination: "Prometheus/OTLP endpoint (si configurado)".into(),
                encryption: true,
                consent_required: false,
                retention_days: 30,
            },
            DataFlow {
                data_type: "Tokens de API".into(),
                destination: "Solo en memoria (SecretString)".into(),
                encryption: true,
                consent_required: false,
                retention_days: 0,
            },
        ]
    }

    /// Genera el documento de postura de seguridad.
    pub fn generate_posture_document() -> String {
        let threats = Self::threat_model();
        let data_flows = Self::data_flows();

        let mut doc = String::from("# Postura de Seguridad de The Hive\n\n");
        doc.push_str("## 1. Modelo de Amenazas\n\n");
        doc.push_str("| ID | Categoría | Severidad | Descripción | Mitigación |\n");
        doc.push_str("|----|-----------|-----------|-------------|------------|\n");
        for t in &threats {
            doc.push_str(&format!(
                "| {} | {:?} | {:?} | {} | {} |\n",
                t.id, t.category, t.severity, t.description, t.mitigation
            ));
        }

        doc.push_str("\n## 2. Flujos de Datos\n\n");
        doc.push_str("| Dato | Destino | Cifrado | Consentimiento | Retención |\n");
        doc.push_str("|------|---------|---------|----------------|-----------|\n");
        for d in &data_flows {
            doc.push_str(&format!(
                "| {} | {} | {} | {} | {} días |\n",
                d.data_type, d.destination, d.encryption, d.consent_required, d.retention_days
            ));
        }

        doc.push_str("\n## 3. Controles Implementados\n\n");
        doc.push_str(
            "- **Allowlist de comandos MCP**: solo ejecuta comandos de desarrollo seguros\n",
        );
        doc.push_str("- **Validación de paths**: anti-traversal, bloqueo de .git\n");
        doc.push_str("- **SecretString**: tokens redactados en logs, JSON, Debug\n");
        doc.push_str("- **Sandbox Docker**: aislamiento de red por defecto\n");
        doc.push_str("- **HIVE_PROTECT_MAIN**: protege rama principal\n");
        doc.push_str("- **Audit log JSONL**: registro de acciones con timestamp\n");
        doc.push_str("- **SBOM en CI**: supply chain visibility\n");
        doc.push_str("- **Rate limiting**: control de consumo de API\n");

        doc.push_str("\n## 4. Recomendaciones para Producción\n\n");
        doc.push_str("1. Usar `HIVE_PROTECT_MAIN=1` siempre\n");
        doc.push_str("2. Configurar network policies restrictivas en sandbox\n");
        doc.push_str("3. No enviar código a APIs externas sin consentimiento explícito\n");
        doc.push_str("4. Ejecutar `cargo audit` periódicamente\n");
        doc.push_str("5. Revisar audit logs regularmente\n");
        doc.push_str("6. Usar Ollama local para código sensible (no enviar a cloud)\n");

        doc
    }

    /// Verifica que la configuración cumple con la postura de seguridad.
    pub fn audit_config(
        protect_main: bool,
        sandbox_network: &str,
        has_rate_limit: bool,
    ) -> Vec<String> {
        let mut findings = Vec::new();

        if !protect_main {
            findings.push(
                "⚠️  HIVE_PROTECT_MAIN está desactivado — riesgo de escritura en main".into(),
            );
        }
        if sandbox_network != "none" {
            findings.push(format!(
                "⚠️  Sandbox network='{sandbox_network}' — considerar 'none' para máxima seguridad"
            ));
        }
        if !has_rate_limit {
            findings.push("⚠️  Sin rate limiting — riesgo de agotar cuota API".into());
        }

        if findings.is_empty() {
            findings.push("✅ Configuración cumple con la postura de seguridad".into());
        }
        findings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threat_model_has_entries() {
        let threats = SecurityPosture::threat_model();
        assert!(threats.len() >= 8);
        assert!(threats
            .iter()
            .any(|t| t.severity == ThreatSeverity::Critical));
    }

    #[test]
    fn data_flows_documented() {
        let flows = SecurityPosture::data_flows();
        assert!(!flows.is_empty());
        assert!(flows.iter().any(|f| f.encryption));
    }

    #[test]
    fn posture_document_generates() {
        let doc = SecurityPosture::generate_posture_document();
        assert!(doc.contains("Modelo de Amenazas"));
        assert!(doc.contains("Flujos de Datos"));
        assert!(doc.contains("Controles Implementados"));
    }

    #[test]
    fn audit_config_detects_issues() {
        let findings = SecurityPosture::audit_config(false, "bridge", false);
        assert!(findings.len() >= 2);
    }

    #[test]
    fn audit_config_passes_when_secure() {
        let findings = SecurityPosture::audit_config(true, "none", true);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].contains("✅"));
    }
}
