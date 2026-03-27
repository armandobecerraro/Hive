# Hive

Orquestación autónoma de desarrollo (**La Reina**, obreras, Consejo) sobre repositorios Git. El núcleo Rust vive en [`hive_core/`](hive_core/README.md).

## Novedades 2026 — 12 mejoras implementadas

| Mejora | Módulo | Feature | Descripción |
|--------|--------|---------|-------------|
| Sandbox Docker | `sandbox.rs` | `multiagent` | Ejecución aislada por obrera en contenedores |
| GitHub API | `github_api.rs` | `multiagent` | PRs reales, lectura de issues, reviewers |
| MCP Protocol | `mcp.rs` | `multiagent` | Herramientas estandarizadas para agentes |
| DAG paralelo | `task_dag.rs` | `multiagent` | Tareas con dependencias y ejecución paralela |
| OpenTelemetry | `telemetry.rs` | `multiagent` | Observabilidad distribuida (traces/spans) |
| A2A Protocol | `a2a.rs` | `multiagent` | Comunicación entre agentes (Google A2A) |
| Plugins WASM | `wasm_plugins.rs` | `multiagent` | Especialistas cargados como plugins WebAssembly |
| Memoria RAG | `memory.rs` | — | Vector store para contexto acumulado |
| SWE-bench | `swebench.rs` | — | Benchmark de evaluación del enjambre |
| Pair programming | `pair_programming.rs` | `multiagent` | WebSocket interactivo en tiempo real |
| Multi-repo | `multi_repo.rs` | — | Orquestación sobre múltiples repos |
| CI/CD evaluación | `cicd_eval.rs` | — | E2E tests del propio Hive |

### Activar features

```bash
# Solo módulos base (sin dependencias extra)
cargo build -p hive_core

# Con todas las mejoras (sandbox, GitHub, MCP, DAG, telemetry, A2A, WASM, WebSocket)
cargo build -p hive_core --features multiagent

# Feature completo (alias de multiagent)
cargo build -p hive_core --features full
```

## Comprobar todo antes de un commit

```bash
./scripts/verify-all.sh
```

## Demonio y repos "bien" probados

`hive_core -d` (o `HIVE_DAEMON=1`) ejecuta ciclos continuos. Por defecto, tras crear andamiaje greenfield se valida como un buen CLI: `cargo check` / `fmt` / `clippy` / `test` (Rust), y antes de cada MR simulado suele exigirse `cargo test` si hay `Cargo.toml`. Variables: `HIVE_VALIDATE_SCAFFOLD`, `HIVE_RUN_TESTS_BEFORE_MR` (desactivar con `0`/`false`/`no`). Detalle en [`hive_core/README.md`](hive_core/README.md).

## CI

[`.github/workflows/ci.yml`](.github/workflows/ci.yml): tests Rust (macOS + Ubuntu + cobertura), fmt, clippy, extensión VS Code (`npm test`), auditoría `cargo audit`, y **SBOM** CycloneDX (artefacto `sbom-cyclonedx` generado con Syft).

## Seguridad y suministro

- [`SECURITY.md`](SECURITY.md) — política, criterios OWASP e ISO/IEC 25010 (resumen).
- [`docs/SBOM.md`](docs/SBOM.md) — dependencias y SBOM; los JSON oficiales salen del job **sbom** en CI.

## Extensión de editor

[`editors/vscode-hive/`](editors/vscode-hive/README.md) — panel de misión compatible con VS Code, Cursor, Windsurf, etc.
