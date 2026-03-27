# Security Policy

## Gestión de Vulnerabilidades

### Criterios de aceptación OWASP
- **CVE críticos (CVSS ≥ 9.0)**: No fusionar sin análisis de impacto y plan de corrección
- **CVE altos (CVSS 7.0-8.9)**: Corrección en siguiente release (máximo 30 días)
- **CVE medios/bajos**: Revisar en siguiente ciclo de mantenimiento

### Dependencias críticas monitoreadas
| Dependencia | Riesgo | Mitigación |
|-------------|--------|------------|
| git2 / libgit2 | Acceso a repositorios remotos | Solo usar SSH/HTTPS autenticado; validar host keys |
| reqwest | HTTP requests | Tiempo límite; no seguir redirects no autenticados |
| sysinfo | Info del sistema | Solo lectura |

### Superficie de ataque
1. **Ejecución de binario**: La extensión VS Code ejecuta `hive_core` desde ruta configurada
   - **Mitigación**: Validar que la ruta sea absoluta y el binario exista antes de ejecutar
2. **Operaciones Git**: El core opera sobre repositorios
   - **Mitigación**: Usar credenciales con autenticación multifactor (SSH u HTTPS con tokens), no credenciales en texto plano
3. **Entrada de usuario**: Los archivos `hive.request.json` son parseados
   - **Mitigación**: Validación de esquema JSON; no ejecutar código arbitrario

---

## Calidad del Software (ISO/IEC 25010)

### Características de calidad objetivo

| Característica | Objetivo | Métrica / Umbral |
|----------------|----------|------------------|
| **Funcionalidad** | Cumple especificaciones de automatización Git | Tests de integración pasan |
| **Fiabilidad** | Manejo elegante de fallos Git/red | Sin panics; errores retornados como Result |
| **Seguridad** | Verificación de binarios; aislamiento de rutas | Evitar path traversal; validar rutas absolutas al ejecutable |
| **Mantenibilidad** | Código limpio, documentado, testeable | Cobertura ≥ 94%; sin lints |
| **Portabilidad** | Multi-OS (Linux/macOS/Windows) | CI en macOS y Ubuntu; Docker disponible para entornos reproducibles |

### Excepciones documentadas
- `'sysinfo'` en Windows puede variar en precisión de CPU: Aceptado
- Errores de red Git se propagan como errores esperados; no se reintentan automáticamente

### Proceso de medición
- **Tests**: `cargo test`, `npm test`
- **Cobertura**: `cargo llvm-cov` (objetivo 94%)
- **Estáticos**: `cargo clippy`; en la extensión, `npm run compile` (TypeScript)
- **Seguridad**: `cargo audit`, `npm audit` en cada PR

---

## Reportar vulnerabilidades

Para reportar vulnerabilidades de seguridad, abre un issue privado o contacta al mantenedor directamente.

*Actualizado: 2026-03-26*
