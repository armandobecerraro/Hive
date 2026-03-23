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

*(Hay archivos sueltos en `src/` que no forman parte del árbol de módulos publicado; el comportamiento documentado arriba corresponde a lo que exporta `lib.rs`.)*

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

La protección de rama en GitHub/GitLab es **complementaria**: evita pushes accidentales al remoto; esta política controla el comportamiento local del orquestador.

### Extensión Visual Studio Code / Cursor

En el repo hay una extensión en [`../editors/vscode-hive`](../editors/vscode-hive/README.md): panel de misión, generación de `hive.request.json` y ejecución de `hive_core --once` sobre la carpeta abierta. Requiere el binario compilado y, si hace falta, la opción `hive.executablePath` apuntando a `target/release/hive_core`.

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

Tests y cobertura de líneas (≥ 95 %, `main.rs` excluido) en Docker:
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
  --fail-under-lines 95 \
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

## Future Enhancements

1. **Real API Integration**: Connect to GitHub/GitLab APIs for actual PR creation
2. **Machine Learning**: Predictive analysis for better task prioritization
3. **Plugin System**: Extensible agent types for new languages/frameworks
4. **Distributed Mode**: Multiple Queens coordinating across repositories
5. **Advanced Resource Management**: GPU and network resource monitoring

## License

MIT License

## Contributing

This is an autonomous system - contributions should be made through The Hive's own PR workflow!
