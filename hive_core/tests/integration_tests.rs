//! Pruebas de integración contra la API pública del crate `hive_core`.

use hive_core::council::{
    calculate_quality_score, rejection_indicates_work_obsolete, CouncilVerdict, Maintainer,
    MergeRequest,
};
use hive_core::discovery::colonize_and_analyze;
use hive_core::orchestrator::{system_allows_new_instance, ResourcePolicy};
use std::fs;
use tempfile::TempDir;
use uuid::Uuid;

// ── Tests reales del crate ──────────────────────────────────────────────

#[test]
fn test_colonize_and_analyze_creates_report() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        "[package]\nname = \"t\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize");
    assert!(!report.specialists.is_empty());
    assert!(!report.dna.extension_histogram.is_empty());
}

#[test]
fn test_colonize_and_analyze_python_repo() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("main.py"), "print('hello')").unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize python");
    assert!(!report.specialists.is_empty());
    assert!(report.dna.extension_histogram.contains_key("py"));
}

#[test]
fn test_colonize_and_analyze_javascript_repo() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("index.js"), "console.log('hi')").unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize js");
    assert!(!report.specialists.is_empty());
    assert!(report.dna.extension_histogram.contains_key("js"));
}

#[test]
fn test_colonize_and_analyze_detects_todo_markers() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("main.rs"),
        "// TODO: fix this\nfn main() {}",
    )
    .unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize debt");
    assert!(report.dna.debt.todo_markers > 0);
}

#[test]
fn test_colonize_and_analyze_empty_dir() {
    let temp_dir = TempDir::new().unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize empty");
    assert!(report.was_empty_before_init);
}

#[test]
fn test_colonize_and_analyze_sets_repo_root() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize");
    assert_eq!(report.repo_root, temp_dir.path());
}

#[test]
fn test_maintainer_review_cycle() {
    let m = Maintainer::default();
    let mr = MergeRequest {
        id: Uuid::new_v4(),
        branch: "task/revision".into(),
        title: "MR de prueba".into(),
        description: "contexto".into(),
        worker_id: Uuid::new_v4(),
        specialist_key: "rust".into(),
    };
    let tmp = tempfile::tempdir().unwrap();
    let first = m.review(0, &mr, tmp.path());
    assert!(matches!(first.verdict, CouncilVerdict::Rejected { .. }));
    let second = m.review(2, &mr, tmp.path());
    assert!(matches!(second.verdict, CouncilVerdict::Approved));
}

#[test]
fn test_maintainer_rejects_when_tests_fail() {
    let m = Maintainer {
        reject_before_approve: 0,
    };
    let mr = MergeRequest {
        id: Uuid::new_v4(),
        branch: "hive/worker/1".into(),
        title: "Test".into(),
        description: "desc".into(),
        worker_id: Uuid::new_v4(),
        specialist_key: "rust".into(),
    };
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Cargo.toml"), "[invalid").unwrap();
    let r = m.review(1, &mr, tmp.path());
    assert!(matches!(r.verdict, CouncilVerdict::Rejected { .. }));
}

#[test]
fn test_resource_gate_permisive() {
    let policy = ResourcePolicy {
        max_cpu_percent: 100.0,
        min_available_ram_bytes: 0,
    };
    assert!(system_allows_new_instance(&policy));
}

#[test]
fn test_resource_gate_blocks_on_impossible_ram() {
    let policy = ResourcePolicy {
        max_cpu_percent: 100.0,
        min_available_ram_bytes: u64::MAX,
    };
    assert!(!system_allows_new_instance(&policy));
}

#[test]
fn test_quality_score_passing_tests() {
    assert!(calculate_quality_score(true, 0, 5) > 90);
}

#[test]
fn test_quality_score_failing_tests() {
    assert!(calculate_quality_score(false, 0, 5) < 60);
}

#[test]
fn test_quality_score_many_warnings() {
    assert!(calculate_quality_score(true, 50, 5) < 60);
}

#[test]
fn test_rejection_obsolete_detection() {
    assert!(rejection_indicates_work_obsolete(
        "Ya no se requiere este cambio"
    ));
    assert!(rejection_indicates_work_obsolete("NOT NEEDED anymore"));
    assert!(rejection_indicates_work_obsolete("Ya está hecho en main"));
    assert!(rejection_indicates_work_obsolete("duplicate work"));
    assert!(!rejection_indicates_work_obsolete(
        "Rechazo técnico: endurecer mensaje de commit"
    ));
}

#[test]
fn test_worker_task_construction() {
    let specialist = hive_core::discovery::SpecialistProfile {
        language_key: "rust".into(),
        prompt_blueprint: "p".into(),
        suggested_tools: vec![],
        weight: 1.0,
    };
    let task = hive_core::orchestrator::WorkerTask {
        id: Uuid::new_v4(),
        title: "Tarea de prueba".into(),
        specialist,
        branch_mode: hive_core::orchestrator::WorkerBranchMode::DerivedFromMain,
        mission_one_liner: String::new(),
        mission_brief: "brief".into(),
        work_mode: hive_core::request::HiveWorkMode::Build,
    };
    assert_eq!(task.title, "Tarea de prueba");
    assert!(matches!(
        task.branch_mode,
        hive_core::orchestrator::WorkerBranchMode::DerivedFromMain
    ));
}

#[test]
fn test_hive_state_load_or_new() {
    let tmp = TempDir::new().unwrap();
    let state1 = hive_core::state::HiveState::load_or_new(tmp.path()).unwrap();
    assert_eq!(state1.schema_version, 1);
    state1.save(tmp.path()).unwrap();
    let state2 = hive_core::state::HiveState::load_or_new(tmp.path()).unwrap();
    assert_eq!(state2.schema_version, 1);
}

#[test]
fn test_hive_state_decision_log() {
    let tmp = TempDir::new().unwrap();
    let mut state = hive_core::state::HiveState::load_or_new(tmp.path()).unwrap();
    state.log_decision("test_decision", String::from("detalles de prueba"));
    assert!(!state.decision_tree.is_empty());
    assert!(state
        .decision_tree
        .iter()
        .any(|d| d.decision.contains("test_decision")));
}

#[test]
fn test_hive_state_trim_retention() {
    let tmp = TempDir::new().unwrap();
    let mut state = hive_core::state::HiveState::load_or_new(tmp.path()).unwrap();
    for i in 0..50 {
        state.log_decision(format!("decision_{i}"), format!("detail {i}"));
    }
    assert_eq!(state.decision_tree.len(), 50);
    state.trim_retention(10, 10);
    assert_eq!(state.decision_tree.len(), 10);
}

#[test]
fn test_hive_config_timeouts() {
    let cfg = hive_core::config::HiveConfig::default();
    assert_eq!(cfg.worker_timeout_secs, 600);
    assert_eq!(cfg.git_timeout_secs, 120);
}

// ── Tests de creación de repositorios ─────────────────────────────────

#[test]
fn test_colonize_and_analyze_typescript_repo() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("index.ts"), "console.log('hello');").unwrap();
    fs::write(
        temp_dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{}}"#,
    )
    .unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize typescript");
    assert!(!report.specialists.is_empty());
    assert!(report.dna.extension_histogram.contains_key("ts"));
}

#[test]
fn test_colonize_and_analyze_go_repo() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("main.go"),
        "package main\nfunc main() {}",
    )
    .unwrap();
    fs::write(temp_dir.path().join("go.mod"), "module example.com/m\n").unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize go");
    assert!(!report.specialists.is_empty());
    assert!(report.dna.extension_histogram.contains_key("go"));
}

#[test]
fn test_colonize_and_analyze_multi_language_repo() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(temp_dir.path().join("index.js"), "console.log('hi')").unwrap();
    fs::write(temp_dir.path().join("main.py"), "print('hello')").unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize polyglot");
    assert!(report.dna.extension_histogram.contains_key("rs"));
    assert!(report.dna.extension_histogram.contains_key("js"));
    assert!(report.dna.extension_histogram.contains_key("py"));
}

// ── Tests de edición de repositorios ──────────────────────────────────

#[test]
fn test_specialist_profile_creation() {
    let specialist = hive_core::discovery::SpecialistProfile {
        language_key: "rust".into(),
        prompt_blueprint: "Rust developer".into(),
        suggested_tools: vec!["cargo".into(), "clippy".into()],
        weight: 1.0,
    };
    assert_eq!(specialist.language_key, "rust");
    assert_eq!(specialist.suggested_tools.len(), 2);
    assert_eq!(specialist.weight, 1.0);
}

#[test]
fn test_repository_profile_with_security_issues() {
    let profile = hive_core::discovery::RepositoryProfile {
        languages: vec!["rust".into()],
        frameworks: vec!["tokio".into()],
        technical_debt: hive_core::discovery::TechnicalDebtHints {
            todo_markers: 3,
            large_files: 1,
            missing_readme: false,
            missing_license: true,
            sparse_tests: false,
        },
        specialists: vec![],
        security_issues: vec![hive_core::discovery::SecurityIssue {
            severity: hive_core::discovery::Severity::High,
            category: hive_core::discovery::SecurityCategory::HardcodedSecret,
            file: "config.rs".into(),
            line: Some(42),
            description: "API key hardcodeada".into(),
            suggestion: "Usar variables de entorno".into(),
        }],
        code_patterns: vec![],
    };
    assert_eq!(profile.languages.len(), 1);
    assert_eq!(profile.security_issues.len(), 1);
    assert_eq!(profile.technical_debt.todo_markers, 3);
}

#[test]
fn test_technical_debt_detection() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("main.rs"),
        "// TODO: fix this\n// FIXME: refactor this\nfn main() {}\n",
    )
    .unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize debt");
    assert!(
        report.dna.debt.todo_markers >= 1,
        "debe detectar TODO/FIXME"
    );
}

#[test]
fn test_missing_readme_detection() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize sin readme");
    assert!(
        report.dna.debt.missing_readme,
        "debe detectar README faltante"
    );
}

// ── Tests de orquestación ─────────────────────────────────────────────

#[test]
fn test_resource_policy_default() {
    let policy = hive_core::orchestrator::ResourcePolicy::default();
    assert_eq!(policy.max_cpu_percent, 88.0);
    assert_eq!(policy.min_available_ram_bytes, 256 * 1024 * 1024);
}

#[test]
fn test_git_policy_creation() {
    let policy = hive_core::orchestrator::GitPolicy::new(true, "hive/integration".into());
    assert!(policy.protect_main);
    assert_eq!(policy.integration_branch, "hive/integration");
}

#[test]
fn test_version_file_serialization() {
    let vf = hive_core::orchestrator::VersionFile {
        version: "1.2.3".into(),
    };
    let json = serde_json::to_string(&vf).unwrap();
    assert!(json.contains("1.2.3"));
    let parsed: hive_core::orchestrator::VersionFile = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.version, "1.2.3");
}

// ── Tests de discovery avanzados ──────────────────────────────────────

#[test]
fn test_deduce_specialists_for_python() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("app.py"), "print('hello')").unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize python");
    let has_python = report
        .specialists
        .iter()
        .any(|s| s.language_key == "python");
    assert!(has_python, "debe deducir especialista Python");
}

#[test]
fn test_deduce_specialists_for_javascript() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("app.js"), "console.log('hi')").unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize js");
    let has_js = report.specialists.iter().any(|s| s.language_key == "js_ts");
    assert!(has_js, "debe deducir especialista JS/TS");
}

#[test]
fn test_manifest_detection() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        "[package]\nname=\"x\"\n",
    )
    .unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize con manifest");
    assert!(report
        .dna
        .manifest_hits
        .iter()
        .any(|m| m.contains("Cargo.toml")));
}

#[test]
fn test_code_pattern_large_function() {
    let temp_dir = TempDir::new().unwrap();
    let mut large_fn = String::new();
    for i in 0..100 {
        large_fn.push_str(&format!("    let x{i} = {i};\n"));
    }
    let code = format!("fn main() {{\n{large_fn}}}\n");
    fs::write(temp_dir.path().join("main.rs"), &code).unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize large fn");
    let has_large = report.dna.code_patterns.iter().any(|p| {
        matches!(
            p.pattern_type,
            hive_core::discovery::PatternType::LargeFunction
        )
    });
    assert!(has_large, "debe detectar función grande");
}

#[test]
fn test_code_pattern_deep_nesting() {
    let temp_dir = TempDir::new().unwrap();
    let code = r#"
fn main() {
    if true {
        if true {
            if true {
                if true {
                    if true {
                        println!("deep");
                        println!("line2");
                        println!("line3");
                        println!("line4");
                        println!("line5");
                        println!("line6");
                    }
                }
            }
        }
    }
}
"#;
    fs::write(temp_dir.path().join("main.rs"), code).unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize deep nesting");
    let has_deep = report.dna.code_patterns.iter().any(|p| {
        matches!(
            p.pattern_type,
            hive_core::discovery::PatternType::DeepNesting
        )
    });
    assert!(has_deep, "debe detectar anidación profunda");
}
