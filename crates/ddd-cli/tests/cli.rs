use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn init_dry_run_json_lists_manifest_operation() {
    let temp = tempfile::tempdir().unwrap();

    let mut command = Command::cargo_bin("ddd").unwrap();
    command
        .arg("--cwd")
        .arg(temp.path())
        .arg("--dry-run")
        .arg("--format")
        .arg("json")
        .arg("init")
        .arg("sample-app");

    command
        .assert()
        .success()
        .stdout(predicate::str::contains("\"path\": \"ddd.toml\""));
}

#[test]
fn init_writes_basic_project_files() {
    let temp = tempfile::tempdir().unwrap();

    let mut command = Command::cargo_bin("ddd").unwrap();
    command
        .arg("--cwd")
        .arg(temp.path())
        .arg("init")
        .arg("sample-app")
        .arg("--domain")
        .arg("Invoice");

    command.assert().success();

    assert!(temp.path().join("sample-app/ddd.toml").exists());
    assert!(temp
        .path()
        .join("sample-app/src/domain/invoice.rs")
        .exists());
    assert!(temp
        .path()
        .join("sample-app/tests/invoice_domain.rs")
        .exists());
}

#[test]
fn init_uses_cli_version_for_framework_dependency() {
    let temp = tempfile::tempdir().unwrap();

    let mut command = Command::cargo_bin("ddd").unwrap();
    command
        .arg("--cwd")
        .arg(temp.path())
        .arg("init")
        .arg("sample-app");

    command.assert().success();

    let cargo_toml = std::fs::read_to_string(temp.path().join("sample-app/Cargo.toml")).unwrap();
    assert!(cargo_toml.contains(&format!(
        r#"ddd_cqrs_es = {{ version = "{}""#,
        env!("CARGO_PKG_VERSION")
    )));
}

#[test]
fn wasmtime_runtime_is_not_a_cli_option() {
    let temp = tempfile::tempdir().unwrap();

    let mut command = Command::cargo_bin("ddd").unwrap();
    command
        .arg("--cwd")
        .arg(temp.path())
        .arg("init")
        .arg("bad-app")
        .arg("--preset")
        .arg("leptos-wasi")
        .arg("--runtime")
        .arg("wasmtime");

    command
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn capabilities_json_exposes_agent_contract() {
    let mut command = Command::cargo_bin("ddd").unwrap();
    command.arg("capabilities").arg("--json");

    command
        .assert()
        .success()
        .stdout(predicate::str::contains("\"agent_contract\""));
}
