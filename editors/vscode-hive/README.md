# Hive — extensión (VS Code Extension Host)

Panel de misión para el orquestador **hive_core**: escribe la petición, genera `hive.request.json` y ejecuta `hive_core --once` sobre la carpeta del workspace.

## Compatibilidad multi-IDE

Este paquete es una **extensión estándar** del ecosistema **Visual Studio Code** (manifiesto `package.json`, API `@types/vscode`). **No** es un fork del editor: un solo artefacto sirve para todos los productos que cargan el **Extension Host** compatible.

| Entorno | Instalación habitual |
|--------|------------------------|
| **Visual Studio Code** | Marketplace Microsoft o `.vsix` |
| **Cursor** | *Extensions* → *Install from VSIX…* o arrastrar el `.vsix` |
| **Windsurf** | Misma familia Code OSS; suele aceptar `.vsix` o catálogo compatible |
| **VSCodium / Code OSS** | [Open VSX](https://open-vsx.org/) o `.vsix` |
| **GitHub Codespaces** | Extensiones VS Code estándar |
| **Antigravity u otros** | Si el IDE ofrece **instalación de extensiones VS Code** (VSIX / marketplace compatible), esta extensión debe cargarse igual. Si el producto **no** expone ese host, no aplicará (limitación del IDE, no de Hive). |

**Resumen:** si el editor se comporta como “VS Code con extensiones”, Hive encaja; si es un editor sin ese modelo, haría falta otro tipo de integración (CLI, LSP, app aparte).

## Requisitos

1. **Binario `hive_core`** instalado o compilado desde `hive_core/`:
   ```bash
   cd hive_core && cargo build --release
   ```
   La ruta típica es `hive_core/target/release/hive_core`.

2. Abrir la **carpeta del repositorio** donde debe correr La Reina (workspace con raíz clara).

3. Configurar **`hive.executablePath`** si `hive_core` no está en el `PATH` del proceso del editor.

## Pruebas

```bash
cd editors/vscode-hive
npm install
npm test
```

`npm test` compila TypeScript y ejecuta pruebas unitarias (`node:test`) sobre `hiveRunner` (títulos, payload JSON y escritura de `hive.request.json`).

## Instalación (desarrollo)

```bash
cd editors/vscode-hive
npm install
npm run compile
```

- **Desde carpeta:** paleta de comandos → **Install Extension from Location…** (nombre exacto puede variar ligeramente según el IDE) y elige `editors/vscode-hive`.
- **VSIX:** `npx @vscode/vsce package` y luego *Install from VSIX…*.

Publicar en **Open VSX** además del Marketplace de Microsoft mejora el alcance en forks (p. ej. VSCodium).

## Configuración

| Ajuste | Descripción |
|--------|-------------|
| `hive.executablePath` | Ruta absoluta al binario si no está en `PATH` (p. ej. `.../hive_core/target/release/hive_core`). |
| `hive.showTerminalOnRun` | Si es `true`, duplica el comando en una terminal llamada «Hive». |

## Comandos

- **Hive: Abrir panel de misión** — Misión + opciones (stack, modo, `force_scaffold`) y ejecución del ciclo.
- **Hive: Ejecutar ciclo (hive.request.json actual)** — Solo ejecuta el binario; el JSON debe existir ya.

## Notas

- La extensión **no** incluye el motor Rust; solo invoca `hive_core`.
- Variables como `HIVE_PROTECT_MAIN` las hereda el proceso del editor (entorno del sistema o terminal desde la que lanzaste el IDE).
