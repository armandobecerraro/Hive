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
