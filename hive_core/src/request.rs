//! Solicitud de trabajo del engambre (`hive.request.json`, `HIVE_REQUEST`, CLI `--ask`).
//!
//! Esto no sustituye un LLM: define **qué** debe construirse y dispara un **andamiaje determinista**
//! más el ciclo existente de obreros + Consejo. La capa LLM puede leer el mismo JSON más adelante.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

pub const FILENAME: &str = "hive.request.json";

/// Texto libre del **cliente humano**: nueva iteración o correcciones tras revisar el repo.
/// Tras un ciclo exitoso, si se leyó de disco, se archiva bajo `.hive/client_feedback_archive/`.
pub const CLIENT_FEEDBACK_FILENAME: &str = "hive.client_feedback.md";

/// Origen del feedback del cliente en este ciclo (para archivar solo si vino de fichero).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientFeedbackSource {
    None,
    /// `HIVE_CLIENT_FEEDBACK` (no se archiva; con `--daemon` puede aplicarse en cada ciclo).
    Env,
    /// `hive.client_feedback.md` en la raíz del repo.
    File,
}

/// Cómo debe actuar el engambre respecto al repo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HiveWorkMode {
    /// Según solicitud y si ya hay manifest: sin pedido → mejora; con manifest + pedido → mejora; sin manifest + pedido → construcción.
    #[default]
    Auto,
    /// Priorizar implementación / andamiaje acorde a la solicitud.
    Build,
    /// Revisar y refinar lo existente (sin tratar el repo como greenfield).
    Improve,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct HiveRequest {
    pub title: String,
    pub description: String,
    pub stack: ProjectStack,
    /// Sin manifest: generar esqueleto aunque haya otros ficheros en la carpeta.
    pub force_scaffold: bool,
    /// `auto` (default): mejora si ya hay proyecto o no hay solicitud; construcción si hay solicitud y aún no hay manifest del stack.
    pub work_mode: HiveWorkMode,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStack {
    #[default]
    Auto,
    RustBinary,
    PythonApp,
    NodeMinimal,
    /// Flutter / Dart (`pubspec.yaml`, `lib/`).
    #[serde(alias = "flutter", alias = "dart")]
    FlutterApp,
}

impl HiveRequest {
    /// Carga solicitud: archivo en repo → `HIVE_REQUEST_JSON` → `HIVE_REQUEST` (texto libre) → vacío.
    pub fn resolve(repo: &Path) -> Result<Self> {
        let p = repo.join(FILENAME);
        if p.exists() {
            let raw = fs::read_to_string(&p).with_context(|| format!("leer {}", p.display()))?;
            return serde_json::from_str(&raw).context("hive.request.json inválido");
        }
        if let Ok(j) = std::env::var("HIVE_REQUEST_JSON") {
            return serde_json::from_str(&j).context("HIVE_REQUEST_JSON inválido");
        }
        if let Ok(line) = std::env::var("HIVE_REQUEST") {
            return Ok(HiveRequest {
                title: "Solicitud (HIVE_REQUEST)".into(),
                description: line,
                stack: ProjectStack::Auto,
                force_scaffold: false,
                ..Default::default()
            });
        }
        Ok(HiveRequest::default())
    }

    /// No marcar `#[inline]`: el optimizador podría fusionar comprobaciones y anular el filtro
    /// de especialistas en modo mejora sin `hive.request` (ver `orchestrator::start`).
    #[inline(never)]
    pub fn is_actionable(&self) -> bool {
        !self.title.trim().is_empty() || !self.description.trim().is_empty()
    }
}

/// Lee feedback del cliente: primero `HIVE_CLIENT_FEEDBACK`, si no hay, `hive.client_feedback.md`.
pub fn read_client_feedback_text(repo: &Path) -> Result<Option<(String, ClientFeedbackSource)>> {
    if let Ok(s) = std::env::var("HIVE_CLIENT_FEEDBACK") {
        let t = s.trim();
        if !t.is_empty() {
            return Ok(Some((t.to_string(), ClientFeedbackSource::Env)));
        }
    }
    let p = repo.join(CLIENT_FEEDBACK_FILENAME);
    if p.exists() {
        let raw = fs::read_to_string(&p).with_context(|| format!("leer {}", p.display()))?;
        let t = raw.trim();
        if t.is_empty() {
            return Ok(None);
        }
        return Ok(Some((raw, ClientFeedbackSource::File)));
    }
    Ok(None)
}

/// Incorpora el feedback del cliente a la solicitud **en memoria** (no modifica `hive.request.json`).
/// Si no había pedido previo, el feedback convierte el ciclo en una misión accionable orientada a mejora.
pub fn integrate_client_feedback(
    repo: &Path,
    req: &mut HiveRequest,
) -> Result<ClientFeedbackSource> {
    let Some((text, source)) = read_client_feedback_text(repo)? else {
        return Ok(ClientFeedbackSource::None);
    };
    let block = format!("\n\n---\n## Petición del cliente (nueva iteración)\n\n{text}\n");
    if req.is_actionable() {
        req.description.push_str(&block);
    } else {
        req.title = "Revisión solicitada por el cliente".into();
        req.description = text;
        req.work_mode = HiveWorkMode::Improve;
    }
    Ok(source)
}

/// Tras un ciclo **exitoso**, mueve `hive.client_feedback.md` al archivo (evita re-aplicar el mismo texto en `--daemon`).
pub fn archive_client_feedback_file(repo: &Path) -> Result<()> {
    let p = repo.join(CLIENT_FEEDBACK_FILENAME);
    if !p.exists() {
        return Ok(());
    }
    let dir = repo.join(".hive").join("client_feedback_archive");
    fs::create_dir_all(&dir).with_context(|| format!("crear {}", dir.display()))?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let mut dest = dir.join(format!("{stamp}.md"));
    if dest.exists() {
        dest = dir.join(format!("{stamp}_{}.md", Uuid::new_v4()));
    }
    fs::rename(&p, &dest)
        .with_context(|| format!("archivar feedback: {} -> {}", p.display(), dest.display()))?;
    Ok(())
}

/// Resuelve `work_mode` **antes** de `bootstrap_if_needed`: así un repo vacío con solicitud sigue en modo construcción tras el andamiaje.
pub fn effective_work_mode(req: &HiveRequest, repo: &Path) -> HiveWorkMode {
    match req.work_mode {
        HiveWorkMode::Auto => {
            if !req.is_actionable() || manifest_present_for_stack(repo, req) {
                HiveWorkMode::Improve
            } else {
                HiveWorkMode::Build
            }
        }
        m => m,
    }
}

/// Una línea para títulos de tarea (título + primer renglón de descripción).
pub fn mission_one_liner(req: &HiveRequest) -> String {
    if !req.is_actionable() {
        return String::new();
    }
    let t = req.title.trim();
    let d = req.description.trim();
    let first = d.lines().next().unwrap_or("").trim();
    if t.is_empty() {
        first.chars().take(160).collect()
    } else if first.is_empty() {
        t.chars().take(160).collect()
    } else {
        let mut s = format!("{t} — {first}");
        if s.chars().count() > 160 {
            s = s.chars().take(160).collect();
        }
        s
    }
}

/// Texto para MR / Consejo: objetivo explícito o mejora continua.
pub fn mission_brief_markdown(req: &HiveRequest) -> String {
    if req.is_actionable() {
        format!(
            "## Objetivo (hive.request)\n**{}**\n\n{}",
            req.title, req.description
        )
    } else {
        "Sin solicitud en `hive.request.json` (ni `HIVE_REQUEST`): ciclo de **revisión y mejora** del código existente.".into()
    }
}

fn manifest_present_for_stack(repo: &Path, req: &HiveRequest) -> bool {
    let stack = stack_for_repo(repo, req);
    match stack {
        ProjectStack::RustBinary | ProjectStack::Auto => repo.join("Cargo.toml").exists(),
        ProjectStack::PythonApp => {
            repo.join("pyproject.toml").exists() || repo.join("requirements.txt").exists()
        }
        ProjectStack::NodeMinimal => repo.join("package.json").exists(),
        ProjectStack::FlutterApp => repo.join("pubspec.yaml").exists(),
    }
}

/// Título corto a partir de la primera línea de la descripción (p. ej. CLI `--ask`).
pub fn derive_title(desc: &str) -> String {
    let line = desc.lines().next().unwrap_or("Proyecto").trim();
    if line.is_empty() {
        "Proyecto Hive".into()
    } else {
        line.chars().take(72).collect()
    }
}

pub fn parse_stack(s: &str) -> Option<ProjectStack> {
    match s.to_ascii_lowercase().as_str() {
        "auto" => Some(ProjectStack::Auto),
        "rust" | "rust_binary" => Some(ProjectStack::RustBinary),
        "python" | "python_app" => Some(ProjectStack::PythonApp),
        "node" | "node_minimal" | "javascript" => Some(ProjectStack::NodeMinimal),
        "flutter" | "flutter_app" | "dart" => Some(ProjectStack::FlutterApp),
        _ => None,
    }
}

/// Si la solicitud lo pide y el repo aún no tiene manifest, genera un proyecto mínimo ejecutable.
/// Devuelve `true` si se creó o amplió el andamiaje en esta llamada.
pub fn bootstrap_if_needed(repo: &Path, req: &HiveRequest) -> Result<bool> {
    if !req.is_actionable() {
        return Ok(false);
    }
    let stack = stack_for_repo(repo, req);
    if !should_scaffold(repo, req, stack) {
        return Ok(false);
    }
    match stack {
        ProjectStack::RustBinary | ProjectStack::Auto => scaffold_rust(repo, req)?,
        ProjectStack::PythonApp => scaffold_python(repo, req)?,
        ProjectStack::NodeMinimal => scaffold_node(repo, req)?,
        ProjectStack::FlutterApp => scaffold_flutter(repo, req)?,
    }
    Ok(true)
}

/// Stack efectivo según `hive.request` y ficheros presentes (misma lógica que el bootstrap).
pub fn stack_for_repo(repo: &Path, req: &HiveRequest) -> ProjectStack {
    if req.stack != ProjectStack::Auto {
        return req.stack;
    }
    if let Ok(s) = std::env::var("HIVE_STACK") {
        if let Some(st) = parse_stack(&s) {
            return st;
        }
    }
    if repo.join("Cargo.toml").exists() {
        return ProjectStack::RustBinary;
    }
    if repo.join("pyproject.toml").exists() || repo.join("requirements.txt").exists() {
        return ProjectStack::PythonApp;
    }
    if repo.join("package.json").exists() {
        return ProjectStack::NodeMinimal;
    }
    if repo.join("pubspec.yaml").exists() {
        return ProjectStack::FlutterApp;
    }
    ProjectStack::RustBinary
}

fn should_scaffold(repo: &Path, req: &HiveRequest, stack: ProjectStack) -> bool {
    let manifest = match stack {
        ProjectStack::RustBinary | ProjectStack::Auto => repo.join("Cargo.toml").exists(),
        ProjectStack::PythonApp => {
            repo.join("pyproject.toml").exists() || repo.join("requirements.txt").exists()
        }
        ProjectStack::NodeMinimal => repo.join("package.json").exists(),
        ProjectStack::FlutterApp => repo.join("pubspec.yaml").exists(),
    };
    if manifest && !req.force_scaffold {
        return false;
    }
    if req.force_scaffold && !manifest {
        return true;
    }
    !manifest && dir_barren_enough(repo)
}

/// Directorio “vacío” para Hive: solo metadatos (.git, hive.json, solicitud, cachés).
fn dir_barren_enough(root: &Path) -> bool {
    let ignore: &[&str] = &[
        ".git",
        "hive.json",
        FILENAME,
        ".hive",
        "target",
        ".DS_Store",
        ".gitignore",
    ];
    let Ok(rd) = fs::read_dir(root) else {
        return true;
    };
    for e in rd.flatten() {
        let name = e.file_name();
        let n = name.to_string_lossy();
        if ignore.iter().any(|i| n == *i) {
            continue;
        }
        if n.starts_with(".hive_worker_") && n.ends_with(".md") {
            continue;
        }
        return false;
    }
    true
}

fn slug_package_name(title: &str) -> String {
    let mut out: String = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out = out.trim_matches('_').to_string();
    if out.is_empty()
        || !out
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic())
            .unwrap_or(false)
    {
        return "hive_app".into();
    }
    out.truncate(32.min(out.len()));
    out
}

fn escape_comment_block(s: &str) -> String {
    s.replace("*/", "* /")
}

fn scaffold_rust(repo: &Path, req: &HiveRequest) -> Result<()> {
    let pkg = slug_package_name(&req.title);
    let title_esc = escape_comment_block(&req.title);
    let desc_esc = escape_comment_block(&req.description);

    fs::create_dir_all(repo.join("src"))?;

    let cargo = format!(
        r#"[package]
name = "{pkg}"
version = "0.1.0"
edition = "2021"
description = "{title_esc}"

[dependencies]
"#,
        pkg = pkg,
        title_esc = title_esc.replace('"', "'")
    );
    if !repo.join("Cargo.toml").exists() {
        fs::write(repo.join("Cargo.toml"), cargo)?;
    }

    let mut main_rs = format!("/*\n * Solicitud Hive — {}\n *\n", title_esc);
    for line in desc_esc.lines() {
        main_rs.push_str(&format!(" * {}\n", line));
    }
    main_rs.push_str(
        " *\n * Siguiente paso: implementar lo pedido arriba.\n */\n\
fn main() {\n    println!(\"{} v{}\", env!(\"CARGO_PKG_NAME\"), env!(\"CARGO_PKG_VERSION\"));\n}\n\
\n\
#[cfg(test)]\n\
mod tests {\n\
    #[test]\n\
    fn hive_scaffold_smoke() {\n\
        assert!(!env!(\"CARGO_PKG_NAME\").is_empty());\n\
    }\n\
}\n",
    );
    if !repo.join("src").join("main.rs").exists() {
        fs::write(repo.join("src").join("main.rs"), main_rs)?;
    }

    let readme = format!(
        "# {}\n\n## Solicitud\n\n{}\n\n---\n*Generado por The Hive (bootstrap) — sustituye esto cuando el proyecto crezca.*\n",
        req.title,
        req.description
    );
    if !repo.join("README.md").exists() {
        fs::write(repo.join("README.md"), readme)?;
    }

    if !repo.join("version.json").exists() {
        fs::write(
            repo.join("version.json"),
            "{\n  \"version\": \"0.1.0\"\n}\n",
        )?;
    }

    if !repo.join(".gitignore").exists() {
        fs::write(
            repo.join(".gitignore"),
            "/target\n\
# Cargo.lock se versiona en binarios para builds reproducibles (Cargo book).\n\
.DS_Store\n\
.hive_worker_*.md\n",
        )?;
    }

    let spec = format!(
        "# Especificación (hive.request)\n\n## Título\n{}\n\n## Descripción\n{}\n\n## Stack\n`rust_binary`\n",
        req.title, req.description
    );
    fs::create_dir_all(repo.join("docs"))?;
    fs::write(repo.join("docs").join("HIVE_SPEC.md"), spec)?;

    run_cargo_fmt_after_rust_scaffold(repo)?;
    tracing::info!(package = %pkg, "bootstrap Rust: proyecto mínimo creado desde la solicitud");
    Ok(())
}

/// Normaliza `src/main.rs` con rustfmt para que `cargo fmt --check` pase en validación.
fn run_cargo_fmt_after_rust_scaffold(repo: &Path) -> Result<()> {
    if !repo.join("Cargo.toml").exists() {
        return Ok(());
    }
    let out = Command::new("cargo")
        .current_dir(repo)
        .args(["fmt", "--all"])
        .output()
        .context("cargo fmt tras bootstrap (¿cargo en PATH?)")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("cargo fmt tras bootstrap falló:\n{stderr}");
    }
    Ok(())
}

fn scaffold_python(repo: &Path, req: &HiveRequest) -> Result<()> {
    let title_esc = req.title.replace('"', "'");
    if !repo.join("pyproject.toml").exists() {
        let toml = format!(
            "[project]\nname = \"{}\"\nversion = \"0.1.0\"\ndescription = \"{}\"\nrequires-python = \">=3.10\"\n",
            slug_package_name(&req.title).replace('_', "-"),
            title_esc
        );
        fs::write(repo.join("pyproject.toml"), toml)?;
    }
    if !repo.join("main.py").exists() {
        let py = format!(
            "#!/usr/bin/env python3\n\"\"\"\n{}\n\n{}\n\"\"\"\n\ndef main() -> None:\n    print(\"hive_app\", \"0.1.0\")\n    print(\"Objetivo:\", \"\"\"{}\"\"\")\n\n\nif __name__ == \"__main__\":\n    main()\n",
            req.title, req.description, req.description.replace('\"', "'")
        );
        fs::write(repo.join("main.py"), py)?;
    }
    if !repo.join("README.md").exists() {
        fs::write(
            repo.join("README.md"),
            format!("# {}\n\n{}\n", req.title, req.description),
        )?;
    }
    fs::create_dir_all(repo.join("docs"))?;
    fs::write(
        repo.join("docs").join("HIVE_SPEC.md"),
        format!(
            "# Especificación\n\n## Título\n{}\n\n## Descripción\n{}\n\n## Stack\n`python_app`\n",
            req.title, req.description
        ),
    )?;
    tracing::info!("bootstrap Python: pyproject + main.py");
    Ok(())
}

fn scaffold_node(repo: &Path, req: &HiveRequest) -> Result<()> {
    let name = slug_package_name(&req.title);
    if !repo.join("package.json").exists() {
        let j = serde_json::json!({
            "name": name,
            "version": "0.1.0",
            "description": req.title,
            "main": "index.js",
            "scripts": { "start": "node index.js" }
        });
        fs::write(
            repo.join("package.json"),
            serde_json::to_string_pretty(&j)
                .map_err(|e| anyhow::anyhow!("serializar package.json: {e}"))?,
        )?;
    }
    if !repo.join("index.js").exists() {
        let js = format!(
            "// {}\n// Objetivo detallado: docs/HIVE_SPEC.md\n\nconsole.log('package', '0.1.0');\n",
            req.title.replace(['\n', '\r'], " ")
        );
        fs::write(repo.join("index.js"), js)?;
    }
    if !repo.join("README.md").exists() {
        fs::write(
            repo.join("README.md"),
            format!("# {}\n\n{}\n", req.title, req.description),
        )?;
    }
    fs::create_dir_all(repo.join("docs"))?;
    fs::write(
        repo.join("docs").join("HIVE_SPEC.md"),
        format!(
            "# Especificación\n\n## Título\n{}\n\n## Descripción\n{}\n\n## Stack\n`node_minimal`\n",
            req.title, req.description
        ),
    )?;
    tracing::info!("bootstrap Node: package.json + index.js");
    Ok(())
}

/// Esqueleto Flutter: `pubspec.yaml`, `lib/`, `test/`, y si `flutter` está en PATH tras `pub get`
/// intenta `flutter create .` para añadir `android/`, `ios/`, etc. (simulador / dispositivo sin pasos manuales).
fn scaffold_flutter(repo: &Path, req: &HiveRequest) -> Result<()> {
    let pkg = slug_package_name(&req.title);
    let title_esc = req.title.replace('"', "'");
    if !repo.join("pubspec.yaml").exists() {
        let pubspec = format!(
            r#"name: {pkg}
description: "{title_esc}"
publish_to: 'none'
version: 0.1.0+1

environment:
  sdk: '>=3.0.0 <4.0.0'

dependencies:
  flutter:
    sdk: flutter

dev_dependencies:
  flutter_test:
    sdk: flutter
  flutter_lints: ^5.0.0

flutter:
  uses-material-design: true
"#,
            pkg = pkg,
            title_esc = title_esc.replace('\n', " ")
        );
        fs::write(repo.join("pubspec.yaml"), pubspec)?;
    }

    fs::create_dir_all(repo.join("lib"))?;
    if !repo.join("lib").join("main.dart").exists() {
        let main_dart = format!(
            r#"import 'package:flutter/material.dart';

/// Solicitud Hive: {title}
/// {desc}
void main() => runApp(const HiveBootstrapApp());

class HiveBootstrapApp extends StatelessWidget {{
  const HiveBootstrapApp({{super.key}});

  @override
  Widget build(BuildContext context) {{
    return MaterialApp(
      title: '{title_short}',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: Colors.deepPurple),
        useMaterial3: true,
      ),
      home: const _HomePage(),
    );
  }}
}}

class _HomePage extends StatelessWidget {{
  const _HomePage();

  @override
  Widget build(BuildContext context) {{
    return Scaffold(
      appBar: AppBar(title: const Text('Hola mundo')),
      body: const Center(
        child: Text(
          'Hola mundo',
          style: TextStyle(fontSize: 24),
        ),
      ),
    );
  }}
}}
"#,
            title = req.title.replace('\n', " ").replace('{', "(").replace('}', ")"),
            desc = req.description.replace('\n', " ").replace('{', "(").replace('}', ")"),
            title_short = req
                .title
                .chars()
                .take(40)
                .collect::<String>()
                .replace('\'', "")
                .replace('{', "(")
                .replace('}', ")"),
        );
        fs::write(repo.join("lib").join("main.dart"), main_dart)?;
    }

    fs::create_dir_all(repo.join("test"))?;
    if !repo.join("test").join("widget_test.dart").exists() {
        let test_dart = r#"import 'package:flutter_test/flutter_test.dart';

void main() {
  test('placeholder', () {
    expect(1 + 1, 2);
  });
}
"#;
        fs::write(repo.join("test").join("widget_test.dart"), test_dart)?;
    }

    if !repo.join("analysis_options.yaml").exists() {
        fs::write(
            repo.join("analysis_options.yaml"),
            "include: package:flutter_lints/flutter.yaml\n",
        )?;
    }

    if !repo.join(".gitignore").exists() {
        fs::write(
            repo.join(".gitignore"),
            "# Flutter/Dart\n\
.dart_tool/\n\
.flutter-plugins-dependencies\n\
build/\n\
*.iml\n\
.DS_Store\n\
.hive_worker_*.md\n",
        )?;
    }

    fs::create_dir_all(repo.join("docs"))?;
    fs::write(
        repo.join("docs").join("HIVE_SPEC.md"),
        format!(
            "# Especificación\n\n## Título\n{}\n\n## Descripción\n{}\n\n## Stack\n`flutter_app`\n",
            req.title, req.description
        ),
    )?;

    let flutter_ok = Command::new("flutter")
        .current_dir(repo)
        .args(["pub", "get"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if flutter_ok {
        tracing::info!(package = %pkg, "bootstrap Flutter: pubspec + lib/; flutter pub get ok");
        try_flutter_create_platforms(repo, &pkg);
    } else {
        tracing::info!(
            package = %pkg,
            "bootstrap Flutter: pubspec + lib/ (ejecuta `flutter pub get` cuando tengas el SDK)"
        );
    }

    Ok(())
}

/// Añade carpetas de plataforma (`android`, `ios`, …) para poder ejecutar en simulador o dispositivo.
/// No falla el andamiaje si el comando falla (red, licencias, etc.).
fn try_flutter_create_platforms(repo: &Path, project_name: &str) {
    if repo.join("android").is_dir() && repo.join("ios").is_dir() {
        tracing::info!(
            path = %repo.display(),
            "bootstrap Flutter: plataformas ya presentes; omito flutter create"
        );
        return;
    }
    let out = Command::new("flutter")
        .current_dir(repo)
        .args(["create", ".", "--project-name", project_name])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            tracing::info!(
                path = %repo.display(),
                "bootstrap Flutter: flutter create . (plataformas para simulador/dispositivo)"
            );
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            let stdout = String::from_utf8_lossy(&o.stdout);
            tracing::warn!(
                path = %repo.display(),
                stderr = %stderr,
                stdout = %stdout,
                "bootstrap Flutter: flutter create . falló; puedes ejecutarlo a mano en el repo"
            );
        }
        Err(e) => {
            tracing::warn!(
                path = %repo.display(),
                error = %e,
                "bootstrap Flutter: no se pudo ejecutar flutter create ."
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::tempdir;

    #[test]
    fn parse_stack_variants() {
        assert_eq!(parse_stack("RUST_BINARY"), Some(ProjectStack::RustBinary));
        assert_eq!(parse_stack("python"), Some(ProjectStack::PythonApp));
        assert_eq!(parse_stack("node_minimal"), Some(ProjectStack::NodeMinimal));
        assert_eq!(parse_stack("flutter_app"), Some(ProjectStack::FlutterApp));
        assert_eq!(parse_stack("dart"), Some(ProjectStack::FlutterApp));
        assert_eq!(parse_stack("auto"), Some(ProjectStack::Auto));
        assert_eq!(parse_stack("nope"), None);
    }

    #[test]
    fn derive_title_truncates() {
        let s = "a".repeat(100);
        assert_eq!(derive_title(&s).len(), 72);
    }

    #[test]
    #[serial]
    fn resolve_from_hive_request_env() {
        let t = tempdir().unwrap();
        std::env::set_var("HIVE_REQUEST", "Generar CLI de notas");
        let r = HiveRequest::resolve(t.path()).unwrap();
        std::env::remove_var("HIVE_REQUEST");
        assert!(r.description.contains("CLI"));
    }

    #[test]
    #[serial]
    fn resolve_from_hive_request_json_env() {
        let t = tempdir().unwrap();
        let j = r#"{"title":"T","description":"D","stack":"python_app","force_scaffold":false}"#;
        std::env::set_var("HIVE_REQUEST_JSON", j);
        let r = HiveRequest::resolve(t.path()).unwrap();
        std::env::remove_var("HIVE_REQUEST_JSON");
        assert_eq!(r.stack, ProjectStack::PythonApp);
    }

    #[test]
    #[serial]
    fn resolve_empty_without_file() {
        std::env::remove_var("HIVE_REQUEST");
        std::env::remove_var("HIVE_REQUEST_JSON");
        let t = tempdir().unwrap();
        let r = HiveRequest::resolve(t.path()).unwrap();
        assert!(!r.is_actionable());
    }

    #[test]
    fn bootstrap_rust_from_json_file() {
        let t = tempdir().unwrap();
        let req = HiveRequest {
            title: "Mi API".into(),
            description: "Servicio REST de libros con validación.".into(),
            stack: ProjectStack::RustBinary,
            force_scaffold: false,
            ..Default::default()
        };
        fs::write(
            t.path().join(FILENAME),
            serde_json::to_string(&req).unwrap(),
        )
        .unwrap();
        let loaded = HiveRequest::resolve(t.path()).unwrap();
        bootstrap_if_needed(t.path(), &loaded).unwrap();
        assert!(t.path().join("Cargo.toml").exists());
        assert!(t.path().join("src/main.rs").exists());
        let main = fs::read_to_string(t.path().join("src/main.rs")).unwrap();
        assert!(main.contains("Servicio REST"));
    }

    #[test]
    fn no_bootstrap_when_cargo_exists() {
        let t = tempdir().unwrap();
        fs::write(
            t.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        let req = HiveRequest {
            title: "x".into(),
            description: "y".into(),
            stack: ProjectStack::RustBinary,
            force_scaffold: false,
            ..Default::default()
        };
        bootstrap_if_needed(t.path(), &req).unwrap();
        assert!(!t.path().join("src/main.rs").exists());
    }

    #[test]
    fn bootstrap_python_stack() {
        let t = tempdir().unwrap();
        let req = HiveRequest {
            title: "PyApp".into(),
            description: "Utilidad de línea de comandos".into(),
            stack: ProjectStack::PythonApp,
            force_scaffold: false,
            ..Default::default()
        };
        fs::write(
            t.path().join(FILENAME),
            serde_json::to_string(&req).unwrap(),
        )
        .unwrap();
        let loaded = HiveRequest::resolve(t.path()).unwrap();
        bootstrap_if_needed(t.path(), &loaded).unwrap();
        assert!(t.path().join("pyproject.toml").exists());
        assert!(t.path().join("main.py").exists());
    }

    #[test]
    fn bootstrap_node_stack() {
        let t = tempdir().unwrap();
        let req = HiveRequest {
            title: "Nodo".into(),
            description: "Microservicio mock".into(),
            stack: ProjectStack::NodeMinimal,
            force_scaffold: false,
            ..Default::default()
        };
        fs::write(
            t.path().join(FILENAME),
            serde_json::to_string(&req).unwrap(),
        )
        .unwrap();
        let loaded = HiveRequest::resolve(t.path()).unwrap();
        bootstrap_if_needed(t.path(), &loaded).unwrap();
        assert!(t.path().join("package.json").exists());
        assert!(t.path().join("index.js").exists());
    }

    #[test]
    fn force_scaffold_with_junk_file() {
        let t = tempdir().unwrap();
        fs::write(t.path().join("notas.txt"), "x").unwrap();
        let req = HiveRequest {
            title: "App".into(),
            description: "algo".into(),
            stack: ProjectStack::RustBinary,
            force_scaffold: true,
            ..Default::default()
        };
        bootstrap_if_needed(t.path(), &req).unwrap();
        assert!(t.path().join("Cargo.toml").exists());
    }

    #[test]
    #[serial]
    fn bootstrap_via_hive_request_and_hive_stack_env() {
        let t = tempdir().unwrap();
        std::env::set_var("HIVE_REQUEST", "Generar utilidad Rust");
        std::env::set_var("HIVE_STACK", "rust_binary");
        let r = HiveRequest::resolve(t.path()).unwrap();
        bootstrap_if_needed(t.path(), &r).unwrap();
        std::env::remove_var("HIVE_REQUEST");
        std::env::remove_var("HIVE_STACK");
        assert!(t.path().join("Cargo.toml").exists());
    }

    #[test]
    fn rust_scaffold_preserves_existing_readme_version_gitignore() {
        let t = tempdir().unwrap();
        fs::write(t.path().join("README.md"), "keep").unwrap();
        fs::write(t.path().join("version.json"), "{\"version\":\"9.9.9\"}\n").unwrap();
        fs::write(t.path().join(".gitignore"), "keep\n").unwrap();
        let req = HiveRequest {
            title: "T".into(),
            description: "D".into(),
            stack: ProjectStack::RustBinary,
            force_scaffold: true,
            ..Default::default()
        };
        bootstrap_if_needed(t.path(), &req).unwrap();
        assert_eq!(
            fs::read_to_string(t.path().join("README.md")).unwrap(),
            "keep"
        );
        assert!(fs::read_to_string(t.path().join("version.json"))
            .unwrap()
            .contains("9.9.9"));
    }

    #[test]
    fn bootstrap_noop_when_request_empty() {
        let t = tempdir().unwrap();
        let req = HiveRequest::default();
        assert!(!bootstrap_if_needed(t.path(), &req).unwrap());
        assert!(!t.path().join("Cargo.toml").exists());
    }

    #[test]
    fn resolve_invalid_json_errors() {
        let t = tempdir().unwrap();
        fs::write(t.path().join(FILENAME), "not-json").unwrap();
        assert!(HiveRequest::resolve(t.path()).is_err());
    }

    #[test]
    fn rust_scaffold_sanitizes_comment_block() {
        let t = tempdir().unwrap();
        let req = HiveRequest {
            title: "T".into(),
            description: "nota */ cierre".into(),
            stack: ProjectStack::RustBinary,
            force_scaffold: false,
            ..Default::default()
        };
        bootstrap_if_needed(t.path(), &req).unwrap();
        let m = fs::read_to_string(t.path().join("src/main.rs")).unwrap();
        assert!(m.contains("* /"));
    }

    #[test]
    fn rust_scaffold_skips_existing_main() {
        let t = tempdir().unwrap();
        fs::create_dir_all(t.path().join("src")).unwrap();
        fs::write(t.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        let req = HiveRequest {
            title: "T".into(),
            description: "D".into(),
            stack: ProjectStack::RustBinary,
            force_scaffold: true,
            ..Default::default()
        };
        bootstrap_if_needed(t.path(), &req).unwrap();
        let m = fs::read_to_string(t.path().join("src/main.rs")).unwrap();
        assert_eq!(m.trim(), "fn main() {}");
    }

    #[test]
    fn bootstrap_when_only_hive_json_present() {
        let t = tempdir().unwrap();
        fs::write(t.path().join("hive.json"), "{}").unwrap();
        let req = HiveRequest {
            title: "Solo hive.json".into(),
            description: "proyecto nuevo".into(),
            stack: ProjectStack::RustBinary,
            force_scaffold: false,
            ..Default::default()
        };
        fs::write(
            t.path().join(FILENAME),
            serde_json::to_string(&req).unwrap(),
        )
        .unwrap();
        let loaded = HiveRequest::resolve(t.path()).unwrap();
        bootstrap_if_needed(t.path(), &loaded).unwrap();
        assert!(t.path().join("Cargo.toml").exists());
    }

    #[test]
    fn effective_work_mode_improve_when_no_request() {
        let t = tempdir().unwrap();
        fs::write(
            t.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        let req = HiveRequest::default();
        assert_eq!(effective_work_mode(&req, t.path()), HiveWorkMode::Improve);
    }

    #[test]
    fn effective_work_mode_build_greenfield() {
        let t = tempdir().unwrap();
        let req = HiveRequest {
            title: "App".into(),
            description: "Nueva".into(),
            stack: ProjectStack::RustBinary,
            force_scaffold: false,
            ..Default::default()
        };
        assert_eq!(effective_work_mode(&req, t.path()), HiveWorkMode::Build);
    }

    #[test]
    fn effective_work_mode_improve_when_manifest_exists() {
        let t = tempdir().unwrap();
        fs::write(
            t.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        let req = HiveRequest {
            title: "Feat".into(),
            description: "Añadir módulo".into(),
            stack: ProjectStack::RustBinary,
            force_scaffold: false,
            ..Default::default()
        };
        assert_eq!(effective_work_mode(&req, t.path()), HiveWorkMode::Improve);
    }

    #[test]
    fn effective_work_mode_explicit_build_overrides() {
        let t = tempdir().unwrap();
        fs::write(
            t.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        let req = HiveRequest {
            title: "Feat".into(),
            description: "X".into(),
            stack: ProjectStack::RustBinary,
            force_scaffold: false,
            work_mode: HiveWorkMode::Build,
        };
        assert_eq!(effective_work_mode(&req, t.path()), HiveWorkMode::Build);
    }

    #[test]
    fn mission_one_liner_vacio_sin_solicitud() {
        let req = HiveRequest::default();
        assert!(super::mission_one_liner(&req).is_empty());
    }

    #[test]
    fn mission_one_liner_trunca_y_compone_titulo() {
        let long = "a".repeat(200);
        let solo_titulo = HiveRequest {
            title: long.clone(),
            description: String::new(),
            ..Default::default()
        };
        assert_eq!(super::mission_one_liner(&solo_titulo).len(), 160);

        let primera_larga = format!("{}fin", "c".repeat(200));
        let solo_desc = HiveRequest {
            title: String::new(),
            description: format!("{primera_larga}\nsegunda"),
            ..Default::default()
        };
        assert_eq!(super::mission_one_liner(&solo_desc).len(), 160);

        let ambos = HiveRequest {
            title: "T".into(),
            description: "primera línea\nmás".into(),
            ..Default::default()
        };
        let s = super::mission_one_liner(&ambos);
        assert!(s.starts_with("T — primera"));
    }

    #[test]
    fn mission_brief_markdown_accionable_y_sin_pedido() {
        let acc = HiveRequest {
            title: "T".into(),
            description: "D".into(),
            ..Default::default()
        };
        let m = super::mission_brief_markdown(&acc);
        assert!(m.contains("Objetivo"));
        assert!(m.contains("T"));
        assert!(super::mission_brief_markdown(&HiveRequest::default()).contains("Sin solicitud"));
    }

    #[test]
    #[serial]
    fn integrate_client_feedback_vacio_hace_mision() {
        std::env::remove_var("HIVE_CLIENT_FEEDBACK");
        let t = tempdir().unwrap();
        fs::write(
            t.path().join(super::CLIENT_FEEDBACK_FILENAME),
            "Cambia el README.\n",
        )
        .unwrap();
        let mut req = HiveRequest::default();
        assert_eq!(
            super::integrate_client_feedback(t.path(), &mut req).unwrap(),
            super::ClientFeedbackSource::File
        );
        assert!(req.is_actionable());
        assert!(req.description.contains("README"));
        assert_eq!(req.work_mode, HiveWorkMode::Improve);
    }

    #[test]
    #[serial]
    fn integrate_client_feedback_anexa_a_solicitud_existente() {
        std::env::remove_var("HIVE_CLIENT_FEEDBACK");
        let t = tempdir().unwrap();
        fs::write(t.path().join(super::CLIENT_FEEDBACK_FILENAME), "Más tests.").unwrap();
        let mut req = HiveRequest {
            title: "App".into(),
            description: "Base".into(),
            ..Default::default()
        };
        assert_eq!(
            super::integrate_client_feedback(t.path(), &mut req).unwrap(),
            super::ClientFeedbackSource::File
        );
        assert!(req.description.contains("Base"));
        assert!(req.description.contains("Petición del cliente"));
        assert!(req.description.contains("Más tests"));
    }

    #[test]
    #[serial]
    fn archive_client_feedback_mueve_a_dot_hive() {
        std::env::remove_var("HIVE_CLIENT_FEEDBACK");
        let t = tempdir().unwrap();
        fs::write(t.path().join(super::CLIENT_FEEDBACK_FILENAME), "x").unwrap();
        super::archive_client_feedback_file(t.path()).unwrap();
        assert!(!t.path().join(super::CLIENT_FEEDBACK_FILENAME).exists());
        let arch = t.path().join(".hive/client_feedback_archive");
        assert!(arch.read_dir().unwrap().count() >= 1);
    }

    #[test]
    #[serial]
    fn read_client_feedback_prefiere_variable_de_entorno() {
        std::env::remove_var("HIVE_CLIENT_FEEDBACK");
        std::env::set_var("HIVE_CLIENT_FEEDBACK", "texto desde env");
        let t = tempdir().unwrap();
        fs::write(
            t.path().join(super::CLIENT_FEEDBACK_FILENAME),
            "desde disco",
        )
        .unwrap();
        let (s, src) = super::read_client_feedback_text(t.path()).unwrap().unwrap();
        assert_eq!(s, "texto desde env");
        assert_eq!(src, super::ClientFeedbackSource::Env);
        std::env::remove_var("HIVE_CLIENT_FEEDBACK");
    }

    #[test]
    #[serial]
    fn read_client_feedback_env_solo_espacios_usa_archivo() {
        std::env::remove_var("HIVE_CLIENT_FEEDBACK");
        std::env::set_var("HIVE_CLIENT_FEEDBACK", "  \n\t ");
        let t = tempdir().unwrap();
        fs::write(
            t.path().join(super::CLIENT_FEEDBACK_FILENAME),
            "desde disco",
        )
        .unwrap();
        let (s, src) = super::read_client_feedback_text(t.path()).unwrap().unwrap();
        assert!(s.contains("desde disco"));
        assert_eq!(src, super::ClientFeedbackSource::File);
        std::env::remove_var("HIVE_CLIENT_FEEDBACK");
    }

    #[test]
    fn derive_title_takes_first_line() {
        let title = super::derive_title("First line\nSecond line");
        assert_eq!(title, "First line");
    }

    #[test]
    fn derive_title_empty_returns_proyecto() {
        let title = super::derive_title("");
        assert_eq!(title, "Proyecto");
    }

    #[test]
    fn derive_title_truncates_long_line() {
        let long_line = "x".repeat(100);
        let title = super::derive_title(&long_line);
        assert_eq!(title.len(), 72);
    }

    #[test]
    fn parse_stack_auto() {
        assert_eq!(super::parse_stack("auto"), Some(super::ProjectStack::Auto));
        assert_eq!(super::parse_stack("AUTO"), Some(super::ProjectStack::Auto));
    }

    #[test]
    fn parse_stack_rust() {
        assert_eq!(
            super::parse_stack("rust"),
            Some(super::ProjectStack::RustBinary)
        );
        assert_eq!(
            super::parse_stack("rust_binary"),
            Some(super::ProjectStack::RustBinary)
        );
        assert_eq!(
            super::parse_stack("RUST"),
            Some(super::ProjectStack::RustBinary)
        );
    }

    #[test]
    fn parse_stack_python() {
        assert_eq!(
            super::parse_stack("python"),
            Some(super::ProjectStack::PythonApp)
        );
        assert_eq!(
            super::parse_stack("python_app"),
            Some(super::ProjectStack::PythonApp)
        );
    }

    #[test]
    fn parse_stack_node() {
        assert_eq!(
            super::parse_stack("node"),
            Some(super::ProjectStack::NodeMinimal)
        );
        assert_eq!(
            super::parse_stack("node_minimal"),
            Some(super::ProjectStack::NodeMinimal)
        );
        assert_eq!(
            super::parse_stack("javascript"),
            Some(super::ProjectStack::NodeMinimal)
        );
    }

    #[test]
    fn parse_stack_invalid() {
        assert_eq!(super::parse_stack("invalid"), None);
        assert_eq!(super::parse_stack(""), None);
    }

    #[test]
    #[serial]
    fn read_client_feedback_none_sin_archivos_ni_env() {
        std::env::remove_var("HIVE_CLIENT_FEEDBACK");
        let t = tempdir().unwrap();
        assert!(super::read_client_feedback_text(t.path())
            .unwrap()
            .is_none());
    }

    #[test]
    fn archive_client_feedback_no_op_si_no_existe() {
        let t = tempdir().unwrap();
        super::archive_client_feedback_file(t.path()).unwrap();
        assert!(!t.path().join(super::CLIENT_FEEDBACK_FILENAME).exists());
    }
}
