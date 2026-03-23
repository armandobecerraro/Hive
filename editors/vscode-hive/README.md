# Hive — extensión Visual Studio Code

Panel de misión para el orquestador **hive_core**: escribe la petición, genera `hive.request.json` y ejecuta `hive_core --once` sobre la carpeta del workspace.

## Requisitos

1. **Binario `hive_core`** instalado o compilado desde `hive_core/`:
   ```bash
   cd hive_core && cargo build --release
   ```
   La ruta típica es `hive_core/target/release/hive_core`.

2. Abrir en VS Code / Cursor la **carpeta del repositorio** donde debe correr La Reina (no un archivo suelto sin carpeta).

## Instalación (desarrollo)

```bash
cd editors/vscode-hive
npm install
npm run compile
```

En VS Code: **Run > Install Extension from Location…** y elige `editors/vscode-hive`, o empaqueta con `npx @vscode/vsce package` e instala el `.vsix`.

## Configuración

| Ajuste | Descripción |
|--------|-------------|
| `hive.executablePath` | Ruta absoluta al binario si no está en `PATH` (p. ej. `.../hive_core/target/release/hive_core`). |
| `hive.showTerminalOnRun` | Si es `true`, duplica el comando en una terminal llamada «Hive». |

## Comandos

- **Hive: Abrir panel de misión** — Chat + opciones (stack, modo, `force_scaffold`) y ejecución del ciclo.
- **Hive: Ejecutar ciclo (hive.request.json actual)** — Solo ejecuta el binario; el JSON debe existir ya.

## Notas

- La extensión **no** incluye el motor Rust; solo lo invoca.
- Variables como `HIVE_PROTECT_MAIN` se leen del entorno del proceso del editor (puedes exportarlas antes de abrir VS Code o usar la configuración del sistema).
