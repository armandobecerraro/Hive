# Software Bill of Materials (SBOM)

Este documento lista las dependencias de producción de los componentes de Hive.

## hive_core (Rust)

### Dependencias directas
| Paquete | Versión | Propósito | Tipo de licencia |
|---------|---------|-----------|------------------|
| anyhow | 1.0 | Manejo de errores | MIT/Apache-2.0 |
| chrono | 0.4 | Fechas/tiempo | MIT/Apache-2.0 |
| git2 | 0.19 | Operaciones Git | MPL-2.0 |
| serde | 1.0 | Serialización | MIT/Apache-2.0 |
| serde_json | 1.0 | JSON | MIT/Apache-2.0 |
| sysinfo | 0.32 | Info del sistema | MIT |
| thiserror | 2 | Errores | MIT/Apache-2.0 |
| tokio | 1.38 | Async runtime | MIT/Apache-2.0 |
| tracing | 0.1 | Observabilidad | MIT/Apache-2.0 |
| tracing-subscriber | 0.3 | Logging | MIT/Apache-2.0 |
| uuid | 1.6 | Identificadores | MIT/Apache-2.0 |
| reqwest | 0.12 | HTTP (opcional multiagent) | MIT/Apache-2.0 |

### Dependencias de desarrollo
| Paquete | Versión | Propósito |
|---------|---------|-----------|
| serial_test | 3 | Tests paralelos |
| tempfile | 3 | Archivos temporales |

**Umbral de cobertura**: 94% líneas (solo `main.rs` excluido)

---

## vscode-hive (TypeScript/Node)

### Dependencias de producción
| Paquete | Versión | Propósito |
|---------|---------|-----------|
| vscode | ^1.85.0 | API VS Code |

### Dependencias de desarrollo
| Paquete | Versión | Propósito |
|---------|---------|-----------|
| @types/node | ^20.11.0 | Tipos TS |
| @types/vscode | ^1.85.0 | Tipos VS Code |
| typescript | ^5.3.3 | Compilación |

---

## Cadena de suministro

### Políticas aplicadas
- **Auditorías automatizadas**: `cargo audit` y `npm audit` en CI
- **Actualización de dependencias críticas**:
  - `git2` / libgit2: Actualizar cuando haya CVE relevantes en libgit2 o en OpenSSL (cadena de enlazado)
  - `reqwest`: Monitorear CVEs críticos
  - VS Code API: Mantener compatibilidad con ^1.85.0
- **SBOM máquina-legible**: En CI (job `sbom`) se generan artefactos CycloneDX JSON (`sbom-hive-core.cdx.json`, `sbom-vscode-hive.cdx.json`) con Syft; descargables desde la ejecución del workflow. Esta tabla manual sirve de resumen y puede desactualizarse respecto a `Cargo.lock` / `package-lock.json`; ante duda, prevalece el artefacto del pipeline.

### Criterios de aceptación OWASP
- **CVE críticos (CVSS ≥ 9.0)**: No merges sin análisis de impacto ni plan de fix
- **CVE altos (CVSS 7.0-8.9)**: Corrección en la siguiente release (máx. 30 días)
- **CVE medios/bajos**: Revisar en siguiente ciclo de mantenimiento

---

*Última actualización manual de tablas: 2026-03-26. Los SBOM oficiales en formato CycloneDX JSON se generan en CI (Syft).*
