# The Hive - Autonomous Development Orchestration System

## Overview

The Hive is an autonomous development orchestration system written in Rust. At its core is **The Queen**, an engine designed to colonize, evolve, or create software repositories from scratch. The system is language-agnostic and bases its intelligence on dynamic environment analysis.

## Architecture

### Módulos del crate (código real)

1. **`main.rs`** — Punto de entrada del binario; delega en `hive_core::run_queen_cli`.

2. **`lib.rs`** — API pública: `run_queen_cli` (flujo Reina: colonizar → estado → Consejo → cola de obreras).

3. **`discovery.rs`** — Colonización, escaneo de ADN técnico (extensiones, manifiestos, deuda), `colonize_and_analyze`, perfiles especialistas.

4. **`orchestrator.rs`** — Cola Tokio, política de recursos (`sysinfo`), ramas de trabajo (derivada / huérfana), ciclo MR ↔ Consejo ↔ `main`, Git con **git2**.

5. **`council.rs`** — Mantenedor (`Maintainer`), canales `mpsc` + `oneshot`, revisión de MR y veredicto.

6. **`state.rs`** — Persistencia `hive.json` y decisiones.

7. **`sequential_merger.rs`** — Merge secuencial de ramas con detección de conflictos.

8. **`agent.rs`** — WorkerAgent: ejecución de tareas por especialista.

9. **`request.rs`** — Resolución de `hive.request.json`, bootstrap y scaffolding.

10. **`resource_monitor.rs`** — Monitoreo de CPU/RAM con `sysinfo` (cross-platform).

11. **`git_manager.rs`** — Utilidades Git sobre `libgit2`.

12. **`scaffold_validate.rs`** — Validación post-bootstrap del andamiaje greenfield.

*(Módulos multi-agente legacy bajo `--features multiagent`: `blackboard`, `brain`, `worker`.)*

### Módulos nuevos 2026

| # | Módulo | Feature | Descripción |
|---|--------|---------|-------------|
| 13 | `sandbox.rs` | `multiagent` | Sandbox Docker aislado por obrera (bollard) |
| 14 | `github_api.rs` | `multiagent` | Integración GitHub API real (octocrab) |
| 15 | `mcp.rs` | `multiagent` | Model Context Protocol — herramientas estandarizadas |
| 16 | `task_dag.rs` | `multiagent` | DAG de tareas con ejecución paralela (petgraph) |
| 17 | `telemetry.rs` | `multiagent` | OpenTelemetry observabilidad distribuida |
| 18 | `a2a.rs` | `multiagent` | Agent-to-Agent protocol (Google A2A) |
| 19 | `wasm_plugins.rs` | `multiagent` | Plugins WASM para especialistas dinámicos |
| 20 | `pair_programming.rs` | `multiagent` | WebSocket interactivo para pair programming |
| 21 | `memory.rs` | — | Memoria persistente RAG (vector store local) |
| 22 | `swebench.rs` | — | SWE-bench benchmark de evaluación |
| 23 | `multi_repo.rs` | — | Soporte multi-repo (hive.workspace.json) |
| 24 | `cicd_eval.rs` | — | Evaluación E2E continua del propio Hive |

## Key Features

### Technical DNA Analysis
- Language detection (Rust, Python, JavaScript, Dart, C/C++, etc.)
- Framework identification
- Technical debt detection
- Build tool and CI/CD pipeline recognition

### Autonomous Agent System
- **Dynamic Agent Generation**: The Queen generates specialized agent profiles based on repository analysis
- **Isolated Workspaces**: Each agent works in an orphan Git branch
- **PR-Based Workflow**: Agents create Pull Requests for their changes
- **Feedback Loop**: Council provides review comments, agents can revise

### Resource Management
- **Tokio-based Parallelism**: Efficient async/await concurrency
- **Resource-Aware Scheduling**: Checks system load before spawning agents
- **Queue Management**: Tasks are queued when resources are low

### Persistence
- **hive.json**: Records decision tree and version history
- **Version Tracking**: Maintains history of all changes and decisions
- **State Recovery**: Can resume operations after restart

## Usage

### Basic Execution
```bash
cd hive_core
cargo run -- /path/to/target/directory
```

### Command Line Arguments
- First argument: Target directory path (optional, defaults to current directory)

### Example Workflow
1. **Colonization**: Queen analyzes target directory
2. **DNA Analysis**: Detects technologies and technical debt
3. **Task Generation**: Creates improvement tasks based on analysis
4. **Agent Spawning**: Queen spawns specialized worker agents
5. **Branch Work**: Each agent works in isolated Git branch
6. **PR Creation**: Agents create Pull Requests with changes
7. **Council Review**: Maintainer Council reviews and provides feedback
8. **Merge & Cleanup**: Approved changes are merged, agents are cleaned up

## Configuration

### hive.json Schema
See `examples/hive.json` for the complete schema including:
- Repository profile
- Decision tree
- Version history
- Active agents
- Resource limits
- Council decisions
- System settings

### Variables de entorno (política de ramas)

| Variable | Efecto |
|----------|--------|
| `HIVE_PROTECT_MAIN` | `1` / `true` / `yes`: el enjambre **no** escribe ni fusiona en `main`; merges y hitos van a la rama de integración. |
| `HIVE_INTEGRATION_BRANCH` | Nombre de esa rama (por defecto `main` si `HIVE_PROTECT_MAIN` está desactivado; si está activado y no se define, `hive/integration`). Con `HIVE_PROTECT_MAIN=1` **no** puede ser `main` ni `master` (error al arrancar). |
| `HIVE_VALIDATE_SCAFFOLD` | Por defecto **activado**: tras crear andamiaje greenfield, exige `cargo check`, `fmt --check`, `clippy -D warnings` y `cargo test` (Rust), o `python3 -m py_compile` / `node --check` según stack. Desactivar: `0` / `false` / `no` (p. ej. entorno sin `rustfmt`). |
| `HIVE_RUN_TESTS_BEFORE_MR` | Por defecto **activado**: si existe `Cargo.toml`, ejecuta `cargo test` antes de abrir el MR simulado al Consejo. Desactivar: `0` / `false` / `no`. |

**Daemon recomendado** (ciclos continuos con repos “bien” probados): `HIVE_DAEMON=1` (o `hive_core -d`) y dejar las dos variables anteriores en default; instala toolchain completo (`rustfmt`, `clippy`).

La protección de rama en GitHub/GitLab es **complementaria**: evita pushes accidentales al remoto; esta política controla el comportamiento local del orquestador.

### Extensión (VS Code, Cursor, Windsurf, Open VSX, etc.)

En el repo hay una extensión en [`../editors/vscode-hive`](../editors/vscode-hive/README.md): panel de misión, generación de `hive.request.json` y ejecución de `hive_core --once` sobre la carpeta abierta. Usa el **Extension Host** estándar de VS Code, por lo que aplica a **Cursor**, **Windsurf**, **VSCodium** y cualquier IDE compatible; ver tabla en ese README. Requiere el binario compilado y, si hace falta, `hive.executablePath` → `target/release/hive_core`.

### Cargo.toml Dependencies
- `git2` - Git operations
- `tokio` - Async runtime
- `serde` - Serialization/deserialization
- `uuid` - Unique identifiers
- `chrono` - Timestamps

## Design Patterns

### Strategy Pattern
Used for different language/framework detection and agent specialization.

### Observer Pattern
Council observes agent activities and provides feedback.

### Factory Pattern
Queen dynamically creates specialized agent instances.

## Testing

Tests unitarios e integración:
```bash
cargo test
```

**Escenarios E2E** (`tests/e2e_scenarios.rs`): validan la colmena sobre directorios temporales — **greenfield Rust** (repo desde cero + `cargo check` + **smoke `cargo run`** + **`cargo fmt --check`** + **`cargo clippy -D warnings`** + README / `.gitignore` / `edition`), **segundo ciclo en modo mejora**, **archivo de feedback del cliente** y **greenfield Python** (`python3 -m py_compile main.py` si existe). Requieren `cargo` / `rustfmt` / `clippy` en toolchain (y `python3` para la parte Python); usan `HIVE_SKIP_RESOURCE_GATE=1` para no bloquearse en CPU/RAM.

Solo esos escenarios:
```bash
cargo test e2e_ -- --test-threads=1
```

Tests y cobertura de líneas (≥ 85 %, `main.rs` excluido) en Docker:
```bash
make check
# o
./scripts/docker-check.sh all
```

## Docker

- **Desarrollo / CI en contenedor** (servicio `hive`): compila el proyecto montado en `/app`.
```bash
cd hive_core
docker compose --profile dev build rust-toolchain-tests
docker compose --profile dev run --rm rust-toolchain-tests cargo test --release
docker compose --profile dev run --rm rust-toolchain-tests cargo llvm-cov test --release \
  --fail-under-lines 85 \
  --ignore-filename-regex '^(.*/)?src/main\.rs$' \
  --summary-only
```

- **Imagen de producción** (`hive-prod`, perfil `prod`): binario `hive_core`; monta `./workspace` en `/workspace`.
```bash
docker compose --profile prod build hive-prod
docker compose --profile prod run --rm hive-prod /workspace/mi_repo
```

Atajo que crea `workspace/test_repo` y ejecuta La Reina:
```bash
./scripts/run_in_docker.sh
./scripts/run_in_docker.sh otro_nombre   # usa workspace/otro_nombre
```

## Features (2026)

### Implementados
1. **Sandbox Docker**: Ejecución aislada por obrera en contenedores (`HIVE_USE_SANDBOX=1`)
2. **GitHub API**: PRs reales, lectura de issues, asignación de reviewers (`GITHUB_TOKEN`)
3. **MCP Protocol**: Herramientas estandarizadas (shell, file, git) para agentes
4. **DAG paralelo**: Tareas con dependencias, ejecución por niveles simultáneos
5. **OpenTelemetry**: Traces/spans distribuidos exportados a Jaeger/Tempo (`HIVE_OTEL_ENABLED=1`)
6. **A2A Protocol**: Comunicación entre agentes, descubrimiento de capacidades
7. **WASM Plugins**: Especialistas cargados como módulos WebAssembly
8. **Memoria RAG**: Contexto acumulado entre ciclos (.hive/memory.json)
9. **SWE-bench**: Benchmark de evaluación del enjambre
10. **Pair Programming**: WebSocket interactivo para supervisión humana en tiempo real
11. **Multi-repo**: Orquestación sobre hive.workspace.json
12. **CI/CD eval**: E2E tests del propio Hive con regression testing

### Variables de entorno nuevas

| Variable | Feature | Efecto |
|----------|---------|--------|
| `HIVE_USE_SANDBOX` | `multiagent` | `1`: ejecuta obreras en Docker |
| `GITHUB_TOKEN` | `multiagent` | Token para GitHub API |
| `HIVE_GITHUB_BASE_BRANCH` | `multiagent` | Rama base para PRs (default: `main`) |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | `multiagent` | Endpoint OTLP (default: `localhost:4317`) |
| `HIVE_OTEL_ENABLED` | `multiagent` | `1`: activa OpenTelemetry |
| `HIVE_PAIR_PORT` | — | Puerto WebSocket pair programming |

## License

MIT License

## Contributing

This is an autonomous system - contributions should be made through The Hive's own PR workflow!
