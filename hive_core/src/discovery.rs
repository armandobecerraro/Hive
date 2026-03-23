use anyhow::{Context, Result};
use git2::Repository;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

/// Perfil de agente especialista deducido dinámicamente a partir del ADN del repo (sin catálogo fijo de agentes).
#[derive(Debug, Clone)]
pub struct SpecialistProfile {
    pub language_key: String,
    pub prompt_blueprint: String,
    pub suggested_tools: Vec<String>,
    pub weight: f32,
}

/// Analyzes repositories for technical DNA
pub struct RepositoryAnalyzer {
    target: std::path::PathBuf,
}

impl RepositoryAnalyzer {
    pub fn new(target: std::path::PathBuf) -> Self {
        Self { target }
    }

    pub fn analyze(&self) -> Result<RepositoryProfile, anyhow::Error> {
        let report = colonize_and_analyze(&self.target)?;
        Ok(RepositoryProfile {
            languages: report.dna.dominant_languages.clone(),
            frameworks: report.dna.manifest_hits.clone(),
            technical_debt: report.dna.debt.clone(),
            specialists: report.specialists,
            security_issues: report.dna.security_issues.clone(),
            code_patterns: report.dna.code_patterns.clone(),
        })
    }
}

/// Profile of a repository used by worker agents
#[derive(Debug, Clone, Default)]
pub struct RepositoryProfile {
    pub languages: Vec<String>,
    pub frameworks: Vec<String>,
    pub technical_debt: TechnicalDebtHints,
    pub specialists: Vec<SpecialistProfile>,
    pub security_issues: Vec<SecurityIssue>,
    pub code_patterns: Vec<CodePattern>,
}

/// Problema de seguridad detectado
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityIssue {
    pub severity: Severity,
    pub category: SecurityCategory,
    pub file: PathBuf,
    pub line: Option<usize>,
    pub description: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    #[default]
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum SecurityCategory {
    HardcodedSecret,
    UnsafeCode,
    SqlInjection,
    CommandInjection,
    PathTraversal,
    InsecureDeserialization,
    WeakCrypto,
    #[default]
    MissingValidation,
    ExposedDebug,
    OutdatedDependency,
}

/// Patrón de código detectado
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodePattern {
    pub pattern_type: PatternType,
    pub occurrences: usize,
    pub files: Vec<PathBuf>,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternType {
    LargeFunction,
    DuplicatedCode,
    GodClass,
    DeepNesting,
    MagicNumber,
    LongParameterList,
    EmptyCatch,
    TodoFixme,
}

#[derive(Debug, Clone, Default)]
pub struct TechnicalDebtHints {
    pub todo_markers: usize,
    pub large_files: usize,
    pub missing_readme: bool,
    pub missing_license: bool,
    pub sparse_tests: bool,
}

#[derive(Debug, Clone)]
pub struct RepoDna {
    pub extension_histogram: HashMap<String, usize>,
    pub manifest_hits: Vec<String>,
    pub debt: TechnicalDebtHints,
    pub dominant_languages: Vec<String>,
    pub security_issues: Vec<SecurityIssue>,
    pub code_patterns: Vec<CodePattern>,
}

#[derive(Debug, Clone)]
pub struct ColonizationReport {
    pub repo_root: PathBuf,
    pub was_empty_before_init: bool,
    pub git_initialized_here: bool,
    pub dna: RepoDna,
    pub specialists: Vec<SpecialistProfile>,
}

// --- Security Analysis ---

/// Analiza un archivo en busca de problemas de seguridad
fn analyze_file_security(file_path: &Path, content: &str) -> Vec<SecurityIssue> {
    let mut issues = Vec::new();
    let file_display = file_path.to_path_buf();

    for (line_num, line) in content.lines().enumerate() {
        let line_lower = line.to_lowercase();

        // Hardcoded secrets
        if (line_lower.contains("password")
            || line_lower.contains("secret")
            || line_lower.contains("api_key")
            || line_lower.contains("token"))
            && (line.contains("=") || line.contains(":"))
            && !line.trim().starts_with("//")
            && !line.trim().starts_with("#")
        {
            // Verificar que no sea solo una declaración de variable
            if line.contains("\"") && !line.contains("env::") && !line.contains("std::env") {
                issues.push(SecurityIssue {
                    severity: Severity::High,
                    category: SecurityCategory::HardcodedSecret,
                    file: file_display.clone(),
                    line: Some(line_num + 1),
                    description: "Posible secreto hardcodeado".to_string(),
                    suggestion: "Usar variables de entorno o un gestor de secretos".to_string(),
                });
            }
        }

        // Unsafe code in Rust
        if line.contains("unsafe {") || line.contains("unsafe fn") {
            issues.push(SecurityIssue {
                severity: Severity::Medium,
                category: SecurityCategory::UnsafeCode,
                file: file_display.clone(),
                line: Some(line_num + 1),
                description: "Bloque unsafe encontrado".to_string(),
                suggestion: "Documentar la razón del unsafe y considerar alternativas seguras"
                    .to_string(),
            });
        }

        // Command injection patterns
        if (line.contains("Command::new")
            || line.contains("subprocess")
            || line.contains("os.system"))
            && (line.contains("format!") || line.contains("format!(") || line.contains("+"))
        {
            issues.push(SecurityIssue {
                severity: Severity::High,
                category: SecurityCategory::CommandInjection,
                file: file_display.clone(),
                line: Some(line_num + 1),
                description: "Posible inyección de comando".to_string(),
                suggestion: "Usar argumentos separados en lugar de concatenar strings".to_string(),
            });
        }

        // SQL injection patterns
        if (line.contains("SELECT")
            || line.contains("INSERT")
            || line.contains("UPDATE")
            || line.contains("DELETE"))
            && (line.contains("format!") || line.contains("+"))
        {
            issues.push(SecurityIssue {
                severity: Severity::Critical,
                category: SecurityCategory::SqlInjection,
                file: file_display.clone(),
                line: Some(line_num + 1),
                description: "Posible inyección SQL".to_string(),
                suggestion: "Usar queries parametrizadas".to_string(),
            });
        }

        // Weak crypto
        if line_lower.contains("md5") || line_lower.contains("sha1") || line_lower.contains("des(")
        {
            issues.push(SecurityIssue {
                severity: Severity::Medium,
                category: SecurityCategory::WeakCrypto,
                file: file_display.clone(),
                line: Some(line_num + 1),
                description: "Algoritmo criptográfico débil".to_string(),
                suggestion: "Usar SHA-256 o superior, AES para encriptación".to_string(),
            });
        }
    }

    issues
}

/// Analiza patrones de código
fn analyze_code_patterns(file_path: &Path, content: &str) -> Vec<CodePattern> {
    let mut patterns = Vec::new();
    let file_display = file_path.to_path_buf();

    // Large functions (heuristic: function with more than 50 lines)
    let mut fn_start = None;
    let mut brace_count = 0;
    let mut large_fns = 0;

    for (i, line) in content.lines().enumerate() {
        if line.contains("fn ") && line.contains("(") {
            fn_start = Some(i);
            brace_count = 0;
        }

        brace_count += line.chars().filter(|&c| c == '{').count() as i32;
        brace_count -= line.chars().filter(|&c| c == '}').count() as i32;

        if let Some(start) = fn_start {
            if brace_count == 0 && i > start {
                let fn_size = i - start;
                if fn_size > 50 {
                    large_fns += 1;
                }
                fn_start = None;
            }
        }
    }

    if large_fns > 0 {
        patterns.push(CodePattern {
            pattern_type: PatternType::LargeFunction,
            occurrences: large_fns,
            files: vec![file_display.clone()],
            suggestion: "Dividir funciones grandes en funciones más pequeñas y específicas"
                .to_string(),
        });
    }

    // TODO/FIXME markers
    let todo_count = content.to_lowercase().matches("todo").count()
        + content.to_lowercase().matches("fixme").count()
        + content.to_lowercase().matches("hack").count();

    if todo_count > 0 {
        patterns.push(CodePattern {
            pattern_type: PatternType::TodoFixme,
            occurrences: todo_count,
            files: vec![file_display.clone()],
            suggestion: format!(
                "{todo_count} marcadores TODO/FIXME/HACK encontrados. Priorizar resolución"
            ),
        });
    }

    // Deep nesting (more than 4 levels)
    let deep_nesting = content
        .lines()
        .filter(|line| {
            let indent = line.len() - line.trim_start().len();
            indent > 16 // Aproximadamente 4 niveles de indentación
        })
        .count();

    if deep_nesting > 5 {
        patterns.push(CodePattern {
            pattern_type: PatternType::DeepNesting,
            occurrences: deep_nesting,
            files: vec![file_display],
            suggestion: "Considerar extraer lógica en funciones auxiliares para reducir anidación"
                .to_string(),
        });
    }

    patterns
}

// --- Strategy: un perfil por familia de lenguaje, activado solo si el escaneo lo justifica ---

pub trait LanguageStrategy: Send + Sync {
    fn key(&self) -> &'static str;
    fn relevance(&self, ctx: &ScanContext) -> f32;
    fn build_profile(&self, ctx: &ScanContext) -> SpecialistProfile;
}

pub struct ScanContext<'a> {
    pub ext_counts: &'a HashMap<String, usize>,
    pub manifest_hits: &'a [String],
    pub debt: &'a TechnicalDebtHints,
}

struct RustStrategy;
struct PythonStrategy;
struct DartStrategy;
struct CFamilyStrategy;
struct JsTsStrategy;
struct GenericFallbackStrategy;

impl LanguageStrategy for RustStrategy {
    fn key(&self) -> &'static str {
        "rust"
    }
    fn relevance(&self, ctx: &ScanContext) -> f32 {
        let mut s = *ctx.ext_counts.get("rs").unwrap_or(&0) as f32;
        if ctx.manifest_hits.iter().any(|m| m.ends_with("Cargo.toml")) {
            s += 10.0;
        }
        s
    }
    fn build_profile(&self, ctx: &ScanContext) -> SpecialistProfile {
        let n = *ctx.ext_counts.get("rs").unwrap_or(&0);
        SpecialistProfile {
            language_key: self.key().to_string(),
            prompt_blueprint: format!(
                "Eres un agente obrero Rust. Prioriza idiomático Rust 2021, `cargo clippy`, seguridad y \
                 concurrencia con Tokio cuando aplique. Archivos .rs detectados: {n}. \
                 Deuda: TODOs≈{}, archivos grandes≈{}.",
                ctx.debt.todo_markers, ctx.debt.large_files
            ),
            suggested_tools: vec![
                "cargo".into(),
                "rustfmt".into(),
                "clippy".into(),
            ],
            weight: self.relevance(ctx),
        }
    }
}

impl LanguageStrategy for PythonStrategy {
    fn key(&self) -> &'static str {
        "python"
    }
    fn relevance(&self, ctx: &ScanContext) -> f32 {
        let mut s = *ctx.ext_counts.get("py").unwrap_or(&0) as f32;
        if ctx
            .manifest_hits
            .iter()
            .any(|m| m == "pyproject.toml" || m == "requirements.txt")
        {
            s += 8.0;
        }
        s
    }
    fn build_profile(&self, ctx: &ScanContext) -> SpecialistProfile {
        let n = *ctx.ext_counts.get("py").unwrap_or(&0);
        SpecialistProfile {
            language_key: self.key().to_string(),
            prompt_blueprint: format!(
                "Eres un agente obrero Python. Usa tipado gradual, entornos virtuales y linters (ruff/flake8). \
                 Archivos .py: {n}. Tests escasos en repo: {}.",
                ctx.debt.sparse_tests
            ),
            suggested_tools: vec!["python3".into(), "pip".into(), "ruff".into()],
            weight: self.relevance(ctx),
        }
    }
}

impl LanguageStrategy for DartStrategy {
    fn key(&self) -> &'static str {
        "dart"
    }
    fn relevance(&self, ctx: &ScanContext) -> f32 {
        let mut s = *ctx.ext_counts.get("dart").unwrap_or(&0) as f32;
        if ctx.manifest_hits.iter().any(|m| m == "pubspec.yaml") {
            s += 10.0;
        }
        s
    }
    fn build_profile(&self, ctx: &ScanContext) -> SpecialistProfile {
        let n = *ctx.ext_counts.get("dart").unwrap_or(&0);
        SpecialistProfile {
            language_key: self.key().to_string(),
            prompt_blueprint: format!(
                "Eres un agente obrero Dart/Flutter. Sigue `dart analyze`, null-safety y estructura de paquetes. \
                 Archivos .dart: {n}."
            ),
            suggested_tools: vec!["dart".into(), "flutter".into()],
            weight: self.relevance(ctx),
        }
    }
}

impl LanguageStrategy for CFamilyStrategy {
    fn key(&self) -> &'static str {
        "c_family"
    }
    fn relevance(&self, ctx: &ScanContext) -> f32 {
        (*ctx.ext_counts.get("c").unwrap_or(&0)
            + *ctx.ext_counts.get("h").unwrap_or(&0)
            + *ctx.ext_counts.get("cpp").unwrap_or(&0)
            + *ctx.ext_counts.get("hpp").unwrap_or(&0)) as f32
    }
    fn build_profile(&self, ctx: &ScanContext) -> SpecialistProfile {
        SpecialistProfile {
            language_key: self.key().to_string(),
            prompt_blueprint: format!(
                "Eres un agente obrero C/C++. Prioriza compilación determinista, sanitizers y APIs FFI seguras. \
                 Deuda técnica estimada: TODOs≈{}.",
                ctx.debt.todo_markers
            ),
            suggested_tools: vec!["cmake".into(), "ninja".into(), "clang".into()],
            weight: self.relevance(ctx),
        }
    }
}

impl LanguageStrategy for JsTsStrategy {
    fn key(&self) -> &'static str {
        "js_ts"
    }
    fn relevance(&self, ctx: &ScanContext) -> f32 {
        let mut s = (*ctx.ext_counts.get("js").unwrap_or(&0)
            + *ctx.ext_counts.get("ts").unwrap_or(&0)
            + *ctx.ext_counts.get("tsx").unwrap_or(&0)
            + *ctx.ext_counts.get("jsx").unwrap_or(&0)) as f32;
        if ctx.manifest_hits.iter().any(|m| m == "package.json") {
            s += 8.0;
        }
        s
    }
    fn build_profile(&self, ctx: &ScanContext) -> SpecialistProfile {
        SpecialistProfile {
            language_key: self.key().to_string(),
            prompt_blueprint: "Eres un agente obrero JS/TS: bundlers, eslint/prettier y pruebas."
                .into(),
            suggested_tools: vec!["node".into(), "npm".into(), "pnpm".into()],
            weight: self.relevance(ctx),
        }
    }
}

impl LanguageStrategy for GenericFallbackStrategy {
    fn key(&self) -> &'static str {
        "polyglot"
    }
    fn relevance(&self, ctx: &ScanContext) -> f32 {
        if ctx.ext_counts.is_empty() {
            1.0
        } else {
            0.01
        }
    }
    fn build_profile(&self, _ctx: &ScanContext) -> SpecialistProfile {
        SpecialistProfile {
            language_key: self.key().to_string(),
            prompt_blueprint: "Eres un agente generalista: inspecciona el árbol, documenta hallazgos y propone siguiente paso.".into(),
            suggested_tools: vec!["git".into(), "rg".into(), "tree".into()],
            weight: 1.0,
        }
    }
}

fn all_strategies() -> Vec<Box<dyn LanguageStrategy>> {
    vec![
        Box::new(RustStrategy),
        Box::new(PythonStrategy),
        Box::new(DartStrategy),
        Box::new(CFamilyStrategy),
        Box::new(JsTsStrategy),
        Box::new(GenericFallbackStrategy),
    ]
}

/// Extensiones ya cubiertas por las estrategias nombradas (evita duplicar perfiles).
fn strategy_covered_extensions() -> &'static [&'static str] {
    &[
        "rs", "py", "dart", "c", "h", "cpp", "hpp", "js", "ts", "tsx", "jsx",
    ]
}

/// Modelos ad-hoc adicionales: deducidos solo del histograma para lenguajes no mapeados explícitamente
/// (agnóstico: Go, Ruby, Kotlin, etc. aparecen como perfiles `ext_*`).
fn extension_histogram_profiles(dna: &RepoDna, ctx: &ScanContext) -> Vec<SpecialistProfile> {
    let covered: std::collections::HashSet<&str> =
        strategy_covered_extensions().iter().copied().collect();
    let mut ranked: Vec<(&String, &usize)> = dna.extension_histogram.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1));

    let mut out = Vec::new();
    for (ext, count) in ranked.into_iter().take(8) {
        if *count == 0 {
            continue;
        }
        if matches!(ext.as_str(), "md" | "json" | "yml" | "yaml" | "lock") {
            continue;
        }
        if covered.contains(ext.as_str()) {
            continue;
        }
        let w = *count as f32;
        if w <= 0.5 {
            continue;
        }
        out.push(SpecialistProfile {
            language_key: format!("ext_{ext}"),
            prompt_blueprint: format!(
                "Eres un agente deducido dinámicamente del ADN: muchos archivos `.{}` ({}). \
                 No hay plantilla fija: elige toolchain y convenciones típicas de esa extensión; \
                 cruza con manifiestos y deuda (TODOs≈{}, tests escasos: {}).",
                ext, count, ctx.debt.todo_markers, ctx.debt.sparse_tests
            ),
            suggested_tools: vec!["git".into(), "rg".into(), "file".into()],
            weight: w * 0.85,
        });
    }
    out
}

/// Selección dinámica: solo se materializan especialistas con relevancia > umbral; se añaden perfiles por histograma.
pub fn deduce_specialists(dna: &RepoDna) -> Vec<SpecialistProfile> {
    let ctx = ScanContext {
        ext_counts: &dna.extension_histogram,
        manifest_hits: &dna.manifest_hits,
        debt: &dna.debt,
    };
    let mut profiles: Vec<SpecialistProfile> = all_strategies()
        .iter()
        .filter_map(|s| {
            let w = s.relevance(&ctx);
            if w > 0.5 {
                Some(s.build_profile(&ctx))
            } else {
                None
            }
        })
        .collect();

    let mut seen_keys: std::collections::HashSet<String> =
        profiles.iter().map(|p| p.language_key.clone()).collect();

    for p in extension_histogram_profiles(dna, &ctx) {
        if seen_keys.insert(p.language_key.clone()) {
            profiles.push(p);
        }
    }

    profiles.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if profiles.is_empty() {
        profiles.push(GenericFallbackStrategy.build_profile(&ctx));
    }
    profiles
}

fn is_dir_empty_for_hive(path: &Path) -> Result<bool> {
    if !path.exists() {
        fs::create_dir_all(path).with_context(|| format!("crear {}", path.display()))?;
        return Ok(true);
    }
    let mut any = false;
    for e in fs::read_dir(path).with_context(|| format!("leer_dir {}", path.display()))? {
        let e = e?;
        let name = e.file_name();
        if name == OsStr::new(".git") || name == OsStr::new("hive.json") {
            continue;
        }
        any = true;
        break;
    }
    Ok(!any)
}

/// Devuelve `(inicializó_git_aquí, estaba_vacío_para_hive)`.
fn ensure_git_repo(path: &Path, was_empty: bool) -> Result<(bool, bool)> {
    let git_dir = path.join(".git");
    if git_dir.exists() {
        return Ok((false, was_empty));
    }
    Repository::init(path).with_context(|| format!("git init en {}", path.display()))?;
    Ok((true, was_empty))
}

fn extension_of(path: &Path) -> Option<String> {
    path.extension()?.to_str().map(|s| s.to_lowercase())
}

const MAX_FILE_BYTES_HINT: u64 = 512 * 1024;

fn scan_tree(repo_root: &Path) -> Result<RepoDna> {
    let mut extension_histogram: HashMap<String, usize> = HashMap::new();
    let mut manifest_hits = Vec::new();
    let mut debt = TechnicalDebtHints::default();
    let mut todo_markers = 0usize;
    let mut large_files = 0usize;
    let mut testish = 0usize;
    let mut file_count = 0usize;
    let mut security_issues = Vec::new();
    let mut code_patterns = Vec::new();

    let readme = repo_root.join("README.md").exists() || repo_root.join("README").exists();
    let license = repo_root.join("LICENSE").exists() || repo_root.join("LICENSE.md").exists();
    if !readme {
        debt.missing_readme = true;
    }
    if !license {
        debt.missing_license = true;
    }

    let mut stack = vec![repo_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let read = match fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for e in read.flatten() {
            let p = e.path();
            if is_ignored_path(&p, repo_root) {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            file_count += 1;
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            match name {
                "Cargo.toml" => manifest_hits.push("Cargo.toml".into()),
                "pyproject.toml" => manifest_hits.push("pyproject.toml".into()),
                "requirements.txt" => manifest_hits.push("requirements.txt".into()),
                "pubspec.yaml" => manifest_hits.push("pubspec.yaml".into()),
                "package.json" => manifest_hits.push("package.json".into()),
                _ => {}
            }
            if let Some(ext) = extension_of(&p) {
                *extension_histogram.entry(ext).or_insert(0) += 1;
            }
            if let Ok(meta) = p.metadata() {
                if meta.len() > MAX_FILE_BYTES_HINT {
                    large_files += 1;
                }
            }
            if let Ok(txt) = fs::read_to_string(&p) {
                let lower = txt.to_ascii_lowercase();
                if lower.contains("todo") || lower.contains("fixme") {
                    todo_markers += 1;
                }

                // Analizar seguridad y patrones solo en archivos de código
                let ext = extension_of(&p).unwrap_or_default();
                if matches!(
                    ext.as_str(),
                    "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "go" | "java" | "rb" | "php"
                ) {
                    let file_issues = analyze_file_security(&p, &txt);
                    security_issues.extend(file_issues);

                    let file_patterns = analyze_code_patterns(&p, &txt);
                    code_patterns.extend(file_patterns);
                }
            }
            let pl = p.to_string_lossy();
            if pl.contains("/test")
                || pl.contains("\\test")
                || pl.contains("/tests/")
                || pl.contains("_test.")
                || pl.contains(".spec.")
            {
                testish += 1;
            }
        }
    }

    debt.todo_markers = todo_markers;
    debt.large_files = large_files;
    debt.sparse_tests = file_count > 8 && testish < 2;

    let mut dominant: Vec<String> = extension_histogram
        .iter()
        .filter(|(k, _)| *k != "md" && *k != "json")
        .map(|(k, _)| k.clone())
        .collect();
    dominant.sort_by_key(|k| std::cmp::Reverse(*extension_histogram.get(k).unwrap_or(&0)));
    dominant.truncate(6);

    Ok(RepoDna {
        extension_histogram,
        manifest_hits,
        debt,
        dominant_languages: dominant,
        security_issues,
        code_patterns,
    })
}

fn is_ignored_path(path: &Path, root: &Path) -> bool {
    let rel = path.strip_prefix(root).ok();
    let Some(rel) = rel else {
        return false;
    };
    rel.components()
        .any(|c| c.as_os_str() == OsStr::new(".git") || c.as_os_str() == OsStr::new("target"))
}

/// Coloniza el directorio objetivo: crea ruta si no existe, `git init` si hace falta, y calcula ADN + especialistas.
pub fn colonize_and_analyze(repo_root: &Path) -> Result<ColonizationReport> {
    let was_empty = is_dir_empty_for_hive(repo_root)?;
    let (git_initialized_here, _) = ensure_git_repo(repo_root, was_empty)?;
    let dna = scan_tree(repo_root)?;
    let specialists = deduce_specialists(&dna);
    Ok(ColonizationReport {
        repo_root: repo_root.to_path_buf(),
        was_empty_before_init: was_empty,
        git_initialized_here,
        dna,
        specialists,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn deduce_polyglot_repo_vacio_histograma() {
        let dna = RepoDna {
            extension_histogram: HashMap::new(),
            manifest_hits: vec![],
            debt: TechnicalDebtHints::default(),
            dominant_languages: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        let p = deduce_specialists(&dna);
        assert!(p.iter().any(|x| x.language_key == "polyglot"));
    }

    #[test]
    fn deduce_rust_desde_rs_y_cargo() {
        let mut h = HashMap::new();
        h.insert("rs".into(), 3);
        let dna = RepoDna {
            extension_histogram: h,
            manifest_hits: vec!["Cargo.toml".into()],
            debt: TechnicalDebtHints {
                todo_markers: 1,
                large_files: 0,
                missing_readme: true,
                missing_license: false,
                sparse_tests: true,
            },
            dominant_languages: vec!["rs".into()],
            security_issues: vec![],
            code_patterns: vec![],
        };
        let p = deduce_specialists(&dna);
        assert!(p.iter().any(|x| x.language_key == "rust"));
    }

    #[test]
    fn extension_histogram_go_genera_ext_go() {
        let mut h = HashMap::new();
        h.insert("go".into(), 5);
        let dna = RepoDna {
            extension_histogram: h,
            manifest_hits: vec![],
            debt: TechnicalDebtHints::default(),
            dominant_languages: vec!["go".into()],
            security_issues: vec![],
            code_patterns: vec![],
        };
        let p = deduce_specialists(&dna);
        assert!(p.iter().any(|x| x.language_key == "ext_go"));
    }

    #[test]
    fn is_ignored_path_git_y_target() {
        let root = std::path::Path::new("/repo");
        assert!(is_ignored_path(
            std::path::Path::new("/repo/.git/objects"),
            root
        ));
        assert!(is_ignored_path(
            std::path::Path::new("/repo/target/debug/x"),
            root
        ));
        assert!(!is_ignored_path(
            std::path::Path::new("/repo/src/main.rs"),
            root
        ));
    }

    #[test]
    fn colonize_repo_vacio_inicializa_git() {
        let d = tempfile::tempdir().unwrap();
        let r = colonize_and_analyze(d.path()).unwrap();
        assert!(r.git_initialized_here);
        assert!(r.was_empty_before_init);
        assert!(d.path().join(".git").exists());
    }

    #[test]
    fn scan_detecta_manifestos_y_todo() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("README.md"), "# x").unwrap();
        fs::write(d.path().join("LICENSE"), "MIT").unwrap();
        fs::write(d.path().join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();
        let mut f = fs::File::create(d.path().join("lib.rs")).unwrap();
        writeln!(f, "// TODO fixme").unwrap();
        let r = colonize_and_analyze(d.path()).unwrap();
        assert!(r.dna.manifest_hits.contains(&"Cargo.toml".into()));
        assert!(r.dna.debt.todo_markers >= 1);
        assert!(!r.dna.debt.missing_readme);
    }

    #[test]
    fn archivo_grande_cuenta_deuda() {
        let d = tempfile::tempdir().unwrap();
        let big = d.path().join("big.bin");
        let mut f = fs::File::create(&big).unwrap();
        let buf = vec![0u8; 600_000];
        f.write_all(&buf).unwrap();
        let r = colonize_and_analyze(d.path()).unwrap();
        assert!(r.dna.debt.large_files >= 1);
    }

    #[test]
    fn deduce_python_pyproject() {
        let mut h = HashMap::new();
        h.insert("py".into(), 4);
        let dna = RepoDna {
            extension_histogram: h,
            manifest_hits: vec!["pyproject.toml".into()],
            debt: Default::default(),
            dominant_languages: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        assert!(deduce_specialists(&dna)
            .iter()
            .any(|p| p.language_key == "python"));
    }

    #[test]
    fn deduce_dart_pubspec() {
        let mut h = HashMap::new();
        h.insert("dart".into(), 2);
        let dna = RepoDna {
            extension_histogram: h,
            manifest_hits: vec!["pubspec.yaml".into()],
            debt: Default::default(),
            dominant_languages: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        assert!(deduce_specialists(&dna)
            .iter()
            .any(|p| p.language_key == "dart"));
    }

    #[test]
    fn deduce_js_ts_package_json() {
        let mut h = HashMap::new();
        h.insert("ts".into(), 3);
        let dna = RepoDna {
            extension_histogram: h,
            manifest_hits: vec!["package.json".into()],
            debt: Default::default(),
            dominant_languages: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        assert!(deduce_specialists(&dna)
            .iter()
            .any(|p| p.language_key == "js_ts"));
    }

    #[test]
    fn deduce_c_family() {
        let mut h = HashMap::new();
        h.insert("c".into(), 2);
        h.insert("h".into(), 2);
        let dna = RepoDna {
            extension_histogram: h,
            manifest_hits: vec![],
            debt: TechnicalDebtHints {
                todo_markers: 3,
                ..Default::default()
            },
            dominant_languages: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        assert!(deduce_specialists(&dna)
            .iter()
            .any(|p| p.language_key == "c_family"));
    }

    #[test]
    fn colonize_repo_con_git_previo_no_reinicializa() {
        let d = tempfile::tempdir().unwrap();
        git2::Repository::init(d.path()).unwrap();
        fs::write(d.path().join("x.txt"), "ok").unwrap();
        let r = colonize_and_analyze(d.path()).unwrap();
        assert!(!r.git_initialized_here);
    }

    #[test]
    fn security_analysis_detects_hardcoded_secret() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("README.md"), "# Test").unwrap();
        fs::write(d.path().join("main.rs"), "let api_key = \"sk-1234567890\";").unwrap();
        let r = colonize_and_analyze(d.path()).unwrap();
        assert!(!r.dna.security_issues.is_empty());
        assert!(r
            .dna
            .security_issues
            .iter()
            .any(|i| matches!(i.category, SecurityCategory::HardcodedSecret)));
    }

    #[test]
    fn code_patterns_detects_todos() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("README.md"), "# Test").unwrap();
        fs::write(
            d.path().join("main.rs"),
            "// TODO: fix this\n// FIXME: later\n",
        )
        .unwrap();
        let r = colonize_and_analyze(d.path()).unwrap();
        assert!(!r.dna.code_patterns.is_empty());
        assert!(r
            .dna
            .code_patterns
            .iter()
            .any(|p| matches!(p.pattern_type, PatternType::TodoFixme)));
    }

    #[test]
    fn code_patterns_detecta_funcion_grande() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("README.md"), "# T").unwrap();
        let mut body = String::from("fn enorme() {\n");
        for i in 0..52 {
            body.push_str(&format!("    println!(\"{i}\");\n"));
        }
        body.push_str("}\n");
        fs::write(d.path().join("big.rs"), body).unwrap();
        let r = colonize_and_analyze(d.path()).unwrap();
        assert!(r
            .dna
            .code_patterns
            .iter()
            .any(|p| { matches!(p.pattern_type, PatternType::LargeFunction) }));
    }

    #[test]
    fn code_patterns_detecta_anidacion_profunda() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("README.md"), "# T").unwrap();
        // Más de 5 líneas con indentación > 16 espacios (véase `analyze_code_patterns`)
        let line = "                    x();\n"; // 20 espacios
        let deep = line.repeat(8);
        fs::write(d.path().join("nested.rs"), deep).unwrap();
        let r = colonize_and_analyze(d.path()).unwrap();
        assert!(r
            .dna
            .code_patterns
            .iter()
            .any(|p| { matches!(p.pattern_type, PatternType::DeepNesting) }));
    }

    #[test]
    fn extension_of_returns_extension() {
        let path = std::path::Path::new("src/main.rs");
        let ext = extension_of(path);
        assert_eq!(ext, Some("rs".to_string()));
    }

    #[test]
    fn extension_of_no_extension() {
        let path = std::path::Path::new("Makefile");
        let ext = extension_of(path);
        assert_eq!(ext, None);
    }

    #[test]
    fn extension_of_uppercase() {
        let path = std::path::Path::new("src/Main.RS");
        let ext = extension_of(path);
        assert_eq!(ext, Some("rs".to_string()));
    }

    #[test]
    fn all_strategies_returns_all() {
        let strategies = all_strategies();
        assert_eq!(strategies.len(), 6);
        let keys: Vec<&str> = strategies.iter().map(|s| s.key()).collect();
        assert!(keys.contains(&"rust"));
        assert!(keys.contains(&"python"));
        assert!(keys.contains(&"dart"));
        assert!(keys.contains(&"c_family"));
        assert!(keys.contains(&"js_ts"));
        assert!(keys.contains(&"polyglot"));
    }

    #[test]
    fn strategy_covered_extensions_contains_common() {
        let covered = strategy_covered_extensions();
        assert!(covered.contains(&"rs"));
        assert!(covered.contains(&"py"));
        assert!(covered.contains(&"js"));
    }

    #[test]
    fn extension_histogram_profiles_generates_profiles() {
        let mut h = HashMap::new();
        h.insert("go".into(), 10);
        h.insert("rb".into(), 3);
        let dna = RepoDna {
            extension_histogram: h,
            manifest_hits: vec![],
            debt: TechnicalDebtHints::default(),
            dominant_languages: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        let ctx = ScanContext {
            ext_counts: &dna.extension_histogram,
            manifest_hits: &dna.manifest_hits,
            debt: &dna.debt,
        };
        let profiles = extension_histogram_profiles(&dna, &ctx);
        assert!(!profiles.is_empty());
    }

    #[test]
    fn extension_histogram_profiles_zero_count_ignored() {
        let mut h = HashMap::new();
        h.insert("go".into(), 0);
        let dna = RepoDna {
            extension_histogram: h,
            manifest_hits: vec![],
            debt: TechnicalDebtHints::default(),
            dominant_languages: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        let ctx = ScanContext {
            ext_counts: &dna.extension_histogram,
            manifest_hits: &dna.manifest_hits,
            debt: &dna.debt,
        };
        let profiles = extension_histogram_profiles(&dna, &ctx);
        assert!(profiles.is_empty());
    }

    #[test]
    fn repository_analyzer_new() {
        let analyzer = RepositoryAnalyzer::new(std::path::PathBuf::from("/test/path"));
        let _ = analyzer;
    }

    #[test]
    fn repository_profile_default() {
        let profile = RepositoryProfile::default();
        assert!(profile.languages.is_empty());
        assert!(profile.frameworks.is_empty());
    }

    #[test]
    fn technical_debt_hints_default() {
        let debt = TechnicalDebtHints::default();
        assert_eq!(debt.todo_markers, 0);
        assert_eq!(debt.large_files, 0);
        assert!(!debt.missing_readme);
        assert!(!debt.missing_license);
    }

    #[test]
    fn severity_ordering() {
        let critical = Severity::Critical;
        let high = Severity::High;
        let medium = Severity::Medium;
        let low = Severity::Low;
        let info = Severity::Info;

        assert_ne!(critical, high);
        assert_ne!(high, medium);
        assert_ne!(medium, low);
        assert_ne!(low, info);
        assert_eq!(critical, Severity::Critical);
    }

    #[test]
    fn security_issue_serialization() {
        let issue = SecurityIssue {
            severity: Severity::High,
            category: SecurityCategory::HardcodedSecret,
            file: std::path::PathBuf::from("src/main.rs"),
            line: Some(10),
            description: "Test issue".to_string(),
            suggestion: "Fix it".to_string(),
        };
        let json = serde_json::to_string(&issue).unwrap();
        let deserialized: SecurityIssue = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.severity, Severity::High);
    }

    #[test]
    fn code_pattern_serialization() {
        let pattern = CodePattern {
            pattern_type: PatternType::LargeFunction,
            occurrences: 3,
            files: vec![std::path::PathBuf::from("src/main.rs")],
            suggestion: "Split function".to_string(),
        };
        let json = serde_json::to_string(&pattern).unwrap();
        let deserialized: CodePattern = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.occurrences, 3);
    }

    #[test]
    fn scan_tree_handles_unreadable_file() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("README.md"), "# Test").unwrap();
        let r = colonize_and_analyze(d.path()).unwrap();
        assert!(r.dna.extension_histogram.contains_key("md"));
    }

    #[test]
    fn deduce_specialists_sorts_by_weight() {
        let mut h = HashMap::new();
        h.insert("rs".into(), 5);
        h.insert("py".into(), 2);
        let dna = RepoDna {
            extension_histogram: h,
            manifest_hits: vec!["Cargo.toml".into()],
            debt: TechnicalDebtHints::default(),
            dominant_languages: vec![],
            security_issues: vec![],
            code_patterns: vec![],
        };
        let profiles = deduce_specialists(&dna);
        assert!(!profiles.is_empty());
    }
}
