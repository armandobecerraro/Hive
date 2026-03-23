//! Pruebas de integración del binario / API pública.

use hive_core::run_queen_cli;
use std::path::PathBuf;

#[tokio::test]
async fn run_queen_cli_flujo_completo_en_tempdir() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::env::remove_var("HIVE_BRANCH_MODE");
    // Sin cfg(test) en el crate enlazado, la cola obrero espera CPU/RAM; en CI o Mac cargado no avanza.
    std::env::set_var("HIVE_SKIP_RESOURCE_GATE", "1");

    run_queen_cli(PathBuf::from(tmp.path()))
        .await
        .expect("run_queen_cli");

    assert!(tmp.path().join("hive.json").exists());
    assert!(tmp.path().join(".git").exists());

    std::env::remove_var("HIVE_SKIP_RESOURCE_GATE");
}
