use assert_cmd::Command;
use predicates::prelude::*;

const DBS: &[&str] = &[
    "sqlite", "postgres", "neon", "supabase", "turso", "mysql", "redis",
];
const REALTIMES: &[&str] = &["off", "polling", "redis"];
const TRANSPORTS: &[&str] = &["http", "grpc", "both"];
const PRESETS: &[&str] = &["basic", "leptos-wasi", "native-api", "worker", "custom"];

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

#[test]
fn init_dry_run_accepts_full_spin_runtime_matrix() {
    let temp = tempfile::tempdir().unwrap();

    for db in DBS {
        for realtime in REALTIMES {
            for transport in TRANSPORTS {
                let app_name = format!("app-{db}-{realtime}-{transport}");
                let mut command = Command::cargo_bin("ddd").unwrap();
                command
                    .arg("--cwd")
                    .arg(temp.path())
                    .arg("--dry-run")
                    .arg("--format")
                    .arg("json")
                    .arg("init")
                    .arg(&app_name)
                    .arg("--preset")
                    .arg("leptos-wasi")
                    .arg("--domain")
                    .arg("Counter")
                    .arg("--runtime")
                    .arg("spin")
                    .arg("--db")
                    .arg(db)
                    .arg("--realtime")
                    .arg(realtime)
                    .arg("--transport")
                    .arg(transport)
                    .arg("--ui")
                    .arg("leptos");

                command
                    .assert()
                    .success()
                    .stdout(predicate::str::contains("\"status\": \"planned\""));
            }
        }
    }
}

#[test]
fn each_preset_writes_a_project_that_passes_check() {
    let temp = tempfile::tempdir().unwrap();

    for preset in PRESETS {
        let app_name = format!("preset-{preset}");
        let mut init = Command::cargo_bin("ddd").unwrap();
        init.arg("--cwd")
            .arg(temp.path())
            .arg("init")
            .arg(&app_name)
            .arg("--preset")
            .arg(preset)
            .arg("--domain")
            .arg("Invoice");

        init.assert().success();

        let mut check = Command::cargo_bin("ddd").unwrap();
        check
            .arg("--cwd")
            .arg(temp.path().join(&app_name))
            .arg("check");

        check.assert().success();
    }
}

#[test]
fn add_commands_apply_to_generated_project() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("billing");
    let mut init = Command::cargo_bin("ddd").unwrap();
    init.arg("--cwd")
        .arg(temp.path())
        .arg("init")
        .arg("billing")
        .arg("--preset")
        .arg("basic")
        .arg("--domain")
        .arg("Invoice");
    init.assert().success();

    let add_cases: &[(&str, &[&str])] = &[
        ("aggregate", &["add", "aggregate", "BillingAccount"]),
        (
            "event",
            &[
                "add",
                "event",
                "Invoice",
                "InvoicePaid",
                "--field",
                "amount:i64",
                "--field",
                "paid_at:String",
                "--event-type",
                "invoice_paid",
            ],
        ),
        (
            "command",
            &[
                "add",
                "command",
                "Invoice",
                "PayInvoice",
                "--field",
                "amount:i64",
            ],
        ),
        ("error", &["add", "error", "PaymentError"]),
        ("projection", &["add", "projection", "InvoiceLedger"]),
        ("query", &["add", "query", "InvoiceSummary"]),
        (
            "process-manager",
            &["add", "process-manager", "PaymentSaga"],
        ),
        ("snapshot", &["add", "snapshot", "InvoiceSnapshot"]),
        (
            "upcaster",
            &["add", "upcaster", "InvoicePaid", "--from", "1", "--to", "2"],
        ),
        (
            "route",
            &[
                "add",
                "route",
                "invoice-summary",
                "--method",
                "GET",
                "--path",
                "/api/invoices/summary",
            ],
        ),
        ("grpc-method", &["add", "grpc-method", "PayInvoice"]),
        ("server-fn", &["add", "server-fn", "PayInvoice"]),
        (
            "rest-endpoint",
            &[
                "add",
                "rest-endpoint",
                "invoice-payments",
                "--method",
                "POST",
                "--path",
                "/api/invoices/payments",
            ],
        ),
        ("test", &["add", "test", "invoice-payment"]),
    ];

    for (label, args) in add_cases {
        let mut command = Command::cargo_bin("ddd").unwrap();
        command.arg("--cwd").arg(&project).args(*args);

        command
            .assert()
            .success()
            .stdout(predicate::str::contains("project extension complete"));

        let mut check = Command::cargo_bin("ddd").unwrap();
        check.arg("--cwd").arg(&project).arg("check");
        check.assert().success().stdout(predicate::str::contains(
            "project manifest and generated files are valid",
        ));

        assert!(
            project.join("ddd.toml").exists(),
            "{label} should keep ddd.toml present"
        );
    }
}

#[test]
fn enable_commands_apply_to_generated_project() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("counter");
    let mut init = Command::cargo_bin("ddd").unwrap();
    init.arg("--cwd")
        .arg(temp.path())
        .arg("init")
        .arg("counter")
        .arg("--preset")
        .arg("leptos-wasi")
        .arg("--domain")
        .arg("Counter");
    init.assert().success();

    let enable_cases: &[&[&str]] = &[
        &["enable", "db", "postgres"],
        &["enable", "db", "mysql"],
        &["enable", "db", "redis"],
        &["enable", "redis-store"],
        &["enable", "realtime", "polling"],
        &["enable", "realtime", "redis"],
        &["enable", "grpc"],
        &["enable", "rest"],
        &["enable", "leptos"],
        &["enable", "idempotency"],
        &["enable", "snapshots"],
        &["enable", "tracing"],
    ];

    for args in enable_cases {
        let mut command = Command::cargo_bin("ddd").unwrap();
        command.arg("--cwd").arg(&project).args(*args);
        command
            .assert()
            .success()
            .stdout(predicate::str::contains("capability update complete"));
    }

    let manifest = std::fs::read_to_string(project.join("ddd.toml")).unwrap();
    assert!(manifest.contains("redis-store"));
    assert!(manifest.contains("realtime:redis"));
    assert!(manifest.contains("grpc"));
    assert!(manifest.contains("idempotency"));
    assert!(manifest.contains("snapshots"));
    assert!(manifest.contains("tracing"));

    let cargo_toml = std::fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(cargo_toml.contains("spin-postgres"));
    assert!(cargo_toml.contains("spin-mysql"));
    assert!(cargo_toml.contains("spin-redis"));
    assert!(cargo_toml.contains("tracing"));
}

#[test]
fn runtime_commands_dry_run_for_full_spin_matrix() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("counter");
    let mut init = Command::cargo_bin("ddd").unwrap();
    init.arg("--cwd")
        .arg(temp.path())
        .arg("init")
        .arg("counter")
        .arg("--preset")
        .arg("leptos-wasi")
        .arg("--domain")
        .arg("Counter");
    init.assert().success();

    for db in DBS {
        let mut fresh = Command::cargo_bin("ddd").unwrap();
        fresh
            .arg("--cwd")
            .arg(&project)
            .arg("--dry-run")
            .arg("--format")
            .arg("json")
            .arg("fresh")
            .arg("--db")
            .arg(db);
        fresh
            .assert()
            .success()
            .stdout(predicate::str::contains(r#""fresh""#));

        for realtime in REALTIMES {
            for transport in TRANSPORTS {
                let mut serve = Command::cargo_bin("ddd").unwrap();
                serve
                    .arg("--cwd")
                    .arg(&project)
                    .arg("--dry-run")
                    .arg("--format")
                    .arg("json")
                    .arg("serve")
                    .arg("--runtime")
                    .arg("spin")
                    .arg("--db")
                    .arg(db)
                    .arg("--realtime")
                    .arg(realtime)
                    .arg("--transport")
                    .arg(transport);
                serve
                    .assert()
                    .success()
                    .stdout(predicate::str::contains(r#""make""#))
                    .stdout(predicate::str::contains(r#""spin""#));

                let mut watch = Command::cargo_bin("ddd").unwrap();
                watch
                    .arg("--cwd")
                    .arg(&project)
                    .arg("--dry-run")
                    .arg("--format")
                    .arg("json")
                    .arg("watch")
                    .arg("--runtime")
                    .arg("spin")
                    .arg("--db")
                    .arg(db)
                    .arg("--realtime")
                    .arg(realtime)
                    .arg("--transport")
                    .arg(transport);
                watch
                    .assert()
                    .success()
                    .stdout(predicate::str::contains(r#""cargo""#))
                    .stdout(predicate::str::contains("watch"));
            }
        }
    }
}
