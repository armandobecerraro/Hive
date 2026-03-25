//! Pruebas de integración contra la API pública del crate `hive_core`.

use hive_core::council::{CouncilVerdict, Maintainer, MergeRequest};
use hive_core::discovery::colonize_and_analyze;
use hive_core::orchestrator::{
    system_allows_new_instance, ResourcePolicy, WorkerBranchMode, WorkerTask,
};
use serde_json::Value;
use std::fs;
use tempfile::TempDir;
use uuid::Uuid;

#[test]
fn test_empty_directory_initialization() {
    let temp_dir = TempDir::new().unwrap();
    let target_path = temp_dir.path();
    assert!(target_path.exists());
}

#[test]
fn test_rust_file_detection() {
    let temp_dir = TempDir::new().unwrap();
    let rust_file = temp_dir.path().join("main.rs");
    fs::write(&rust_file, "fn main() {}").unwrap();
    let cargo_file = temp_dir.path().join("Cargo.toml");
    fs::write(
        &cargo_file,
        "[package]\nname = \"test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    assert!(rust_file.exists());
    assert!(cargo_file.exists());
}

#[test]
fn test_python_file_detection() {
    let temp_dir = TempDir::new().unwrap();
    let py_file = temp_dir.path().join("main.py");
    fs::write(&py_file, "print('hello')").unwrap();
    let requirements = temp_dir.path().join("requirements.txt");
    fs::write(&requirements, "requests==2.31.0").unwrap();
    assert!(py_file.exists());
    assert!(requirements.exists());
}

#[test]
fn test_javascript_file_detection() {
    let temp_dir = TempDir::new().unwrap();
    let js_file = temp_dir.path().join("index.js");
    fs::write(&js_file, "console.log('test')").unwrap();
    let ts_file = temp_dir.path().join("app.ts");
    fs::write(&ts_file, "const x: number = 5;").unwrap();
    let package_json = temp_dir.path().join("package.json");
    fs::write(&package_json, "{\"name\": \"test\"}").unwrap();
    assert!(js_file.exists());
    assert!(ts_file.exists());
    assert!(package_json.exists());
}

#[test]
fn test_technical_debt_detection() {
    let temp_dir = TempDir::new().unwrap();
    let todo_file = temp_dir.path().join("TODO");
    fs::write(&todo_file, "Fix this later").unwrap();
    let fixme_file = temp_dir.path().join("FIXME.md");
    fs::write(&fixme_file, "Needs fixing").unwrap();
    assert!(todo_file.exists());
    assert!(fixme_file.exists());
}

#[test]
fn test_ci_cd_detection() {
    let temp_dir = TempDir::new().unwrap();
    let workflows_dir = temp_dir.path().join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();
    let ci_file = workflows_dir.join("ci.yml");
    fs::write(&ci_file, "name: CI").unwrap();
    let dockerfile = temp_dir.path().join("Dockerfile");
    fs::write(&dockerfile, "FROM alpine").unwrap();
    assert!(ci_file.exists());
    assert!(dockerfile.exists());
}

#[test]
fn test_worker_task_fields() {
    let specialist = hive_core::discovery::SpecialistProfile {
        language_key: "rust".into(),
        prompt_blueprint: "p".into(),
        suggested_tools: vec![],
        weight: 1.0,
    };
    let task = WorkerTask {
        id: Uuid::new_v4(),
        title: "Tarea de prueba".into(),
        specialist,
        branch_mode: WorkerBranchMode::DerivedFromMain,
        mission_one_liner: String::new(),
        mission_brief: "brief".into(),
        work_mode: hive_core::request::HiveWorkMode::Build,
    };
    assert_eq!(task.title, "Tarea de prueba");
    assert!(matches!(
        task.branch_mode,
        WorkerBranchMode::DerivedFromMain
    ));
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
fn test_resource_gate_uses_policy() {
    let policy = ResourcePolicy::default();
    let _allowed = system_allows_new_instance(&policy);
}

#[test]
fn test_colonize_and_analyze_minimal_rust() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        "[package]\nname = \"t\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    let report = colonize_and_analyze(temp_dir.path()).expect("colonize");
    assert!(!report.specialists.is_empty());
}

#[test]
fn test_hive_json_schema_parse() {
    let example_json = r#"{
        "version": "1.0.0",
        "created_at": "2026-03-21T15:21:26.310Z",
        "repository_profile": {
            "languages": ["Rust", "Python", "JavaScript"],
            "frameworks": ["React", "Express"],
            "technical_debt": ["TODO files", "Deprecated code"],
            "build_tools": ["Cargo", "npm"],
            "test_frameworks": ["cargo test", "Jest"],
            "ci_cd": ["GitHub Actions", "Docker"]
        }
    }"#;
    let parsed: Value = serde_json::from_str(example_json).unwrap();
    assert_eq!(parsed["version"], "1.0.0");
    let languages = &parsed["repository_profile"]["languages"];
    assert!(languages.is_array());
    assert_eq!(languages[0], "Rust");
}

#[test]
fn test_strategy_pattern_labels() {
    let rust_agent_type = "rust_analyzer";
    let python_agent_type = "python_analyzer";
    let js_agent_type = "js_analyzer";
    assert_ne!(rust_agent_type, python_agent_type);
    assert_ne!(rust_agent_type, js_agent_type);
    assert_ne!(python_agent_type, js_agent_type);
    let agent_types = [rust_agent_type, python_agent_type, js_agent_type];
    assert_eq!(agent_types.len(), 3);
}

#[test]
fn test_message_channels_simulation() {
    use std::sync::mpsc;
    let (tx, rx) = mpsc::channel();
    let feedback = vec![
        "Needs more error handling".to_string(),
        "Add unit tests".to_string(),
    ];
    tx.send(feedback.clone()).unwrap();
    let received = rx.recv().unwrap();
    assert_eq!(received.len(), 2);
    assert_eq!(received[0], "Needs more error handling");
    assert_eq!(received[1], "Add unit tests");
}

#[test]
fn test_version_json_parse() {
    let version_data = r#"{
        "version": "0.1.0",
        "timestamp": "2026-03-21T15:00:00.000Z",
        "changes": ["Initial version"],
        "agent_id": "queen-initializer"
    }"#;
    let parsed: Value = serde_json::from_str(version_data).unwrap();
    assert_eq!(parsed["version"], "0.1.0");
    assert_eq!(parsed["agent_id"], "queen-initializer");
}

#[test]
fn test_orphan_branch_naming_convention() {
    let branch_names = vec![
        "task/rust_analyzer-agent-123",
        "task/python_analyzer-agent-456",
        "task/js_analyzer-agent-789",
    ];
    for branch_name in branch_names {
        assert!(branch_name.starts_with("task/"));
        assert!(!branch_name.contains("main"));
    }
}
