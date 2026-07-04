use assert_cmd::Command;
use predicates::prelude::*;

const DBS: &[&str] = &[
    "sqlite", "postgres", "neon", "supabase", "turso", "mysql", "redis",
];
const REALTIMES: &[&str] = &["off", "polling", "redis"];
const TRANSPORTS: &[&str] = &["http", "grpc", "both"];
const PRESETS: &[&str] = &[
    "basic",
    "leptos-wasi",
    "auth-stack",
    "native-api",
    "worker",
    "custom",
];

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
        .stdout(predicate::str::contains("\"agent_contract\""))
        .stdout(predicate::str::contains("auth-stack"))
        .stdout(predicate::str::contains("oauth:google"));
}

#[test]
fn auth_stack_dry_run_json_lists_auth_template_operations() {
    let temp = tempfile::tempdir().unwrap();

    let mut command = Command::cargo_bin("ddd").unwrap();
    command
        .arg("--cwd")
        .arg(temp.path())
        .arg("--dry-run")
        .arg("--format")
        .arg("json")
        .arg("init")
        .arg("auth-stack")
        .arg("--preset")
        .arg("auth-stack");

    command
        .assert()
        .success()
        .stdout(predicate::str::contains("\"path\": \"spin.toml\""))
        .stdout(predicate::str::contains(
            "\"path\": \"spin.production.toml.example\"",
        ))
        .stdout(predicate::str::contains("\"path\": \"src/app.rs\""))
        .stdout(predicate::str::contains("\"path\": \"src/oauth.rs\""))
        .stdout(predicate::str::contains("\"path\": \"src/rest.rs\""))
        .stdout(predicate::str::contains("\"path\": \"src/grpc.rs\""))
        .stdout(predicate::str::contains("\"path\": \"proto/auth.proto\""))
        .stdout(predicate::str::contains("\"path\": \"proto/authz.proto\""))
        .stdout(predicate::str::contains("\"path\": \".env.example\""));
}

#[test]
fn auth_stack_writes_manifest_defaults_and_passes_check() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("auth-stack");

    let mut init = Command::cargo_bin("ddd").unwrap();
    init.arg("--cwd")
        .arg(temp.path())
        .arg("init")
        .arg("auth-stack")
        .arg("--preset")
        .arg("auth-stack");
    init.assert().success();

    for file in [
        "ddd.toml",
        "Cargo.toml",
        "Makefile",
        "build.rs",
        "input.css",
        "package.json",
        "package-lock.json",
        "spin.toml",
        "spin.production.toml.example",
        ".env.example",
        "src/app.rs",
        "src/application.rs",
        "src/contracts.rs",
        "src/error.rs",
        "src/oauth.rs",
        "src/rest.rs",
        "src/grpc.rs",
        "src/lib.rs",
        "src/main.rs",
        "src/server.rs",
        "src/store.rs",
        "proto/auth.proto",
        "proto/authz.proto",
        "scripts/report_oauth_evidence.sh",
        "scripts/reset_db.sh",
        "scripts/verify_auth_oauth_dev_browser.mjs",
        "scripts/verify_auth_pages.mjs",
        "scripts/verify_auth_passkeys.mjs",
        "scripts/verify_auth_stack.sh",
        "scripts/verify_live_oauth_browser.mjs",
        "scripts/verify_live_oauth_callback.sh",
        "scripts/verify_live_oauth_preflight.sh",
        "scripts/verify_oauth_credentials.sh",
    ] {
        assert!(project.join(file).exists(), "{file} should be generated");
    }
    assert!(
        !project.join("src/domain/mod.rs").exists(),
        "auth-stack should not generate an unused aggregate domain scaffold"
    );

    let manifest = std::fs::read_to_string(project.join("ddd.toml")).unwrap();
    assert!(manifest.contains(r#"preset = "auth-stack""#));
    assert!(manifest.contains(r#"transport = "both""#));
    assert!(manifest.contains(r#"ui = "leptos""#));
    assert!(manifest.contains(r#""auth""#));
    assert!(manifest.contains(r#""authz""#));
    assert!(manifest.contains("[auth]"));
    assert!(manifest.contains("[authz]"));
    assert!(manifest.contains(r#"default_decision = "deny""#));

    let cargo_toml = std::fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(cargo_toml.contains("leptos_wasi"));
    assert!(cargo_toml.contains("spin-sdk"));
    assert!(cargo_toml.contains("ddd-auth"));
    assert!(cargo_toml.contains("ddd-authz"));
    assert!(cargo_toml.contains("[patch.crates-io]"));
    assert!(cargo_toml.contains("ddd_cqrs_es = { path = "));
    assert!(cargo_toml.contains("ddd-auth = { path = "));
    assert!(cargo_toml.contains("crates/ddd-auth"));
    assert!(cargo_toml.contains("ddd-authz = { path = "));
    assert!(cargo_toml.contains("crates/ddd-authz"));
    assert!(cargo_toml.contains("spin-postgres"));
    assert!(cargo_toml.contains("spin-mysql"));

    let makefile = std::fs::read_to_string(project.join("Makefile")).unwrap();
    assert!(makefile.contains("db ?= $(if $(DATABASE_BACKEND),$(DATABASE_BACKEND),sqlite)"));
    assert!(makefile.contains("AUTH_STORAGE_AUTO_CATCH_UP ?= true"));
    assert!(makefile.contains("AUTH_COOKIE_SECURE ?= false"));
    assert!(makefile.contains("--variable auth_cookie_secure=$(AUTH_COOKIE_SECURE)"));
    assert!(makefile.contains("--variable auth_jwt_key_ring_json='$(AUTH_JWT_KEY_RING_JSON)'"));
    assert!(makefile.contains("--variable auth_public_base_url=$(AUTH_PUBLIC_BASE_URL)"));
    assert!(makefile.contains("--variable auth_google_client_secret=$(AUTH_GOOGLE_CLIENT_SECRET)"));
    assert!(makefile.contains("--variable auth_apple_private_key='$(AUTH_APPLE_PRIVATE_KEY)'"));
    assert!(makefile.contains("POSTGRES_URL ?="));
    assert!(makefile.contains("MYSQL_URL ?="));
    assert!(makefile.contains("AUTH_STACK_FEATURES=$(GRPC_FEATURES)"));
    assert!(makefile.contains("oauth-credentials:"));
    assert!(makefile.contains("./scripts/verify_oauth_credentials.sh"));
    assert!(makefile.contains("./scripts/verify_live_oauth_preflight.sh"));
    assert!(makefile.contains("./scripts/report_oauth_evidence.sh"));
    assert!(makefile.contains("npm run browser-smoke"));
    assert!(makefile.contains("npm run passkey-smoke"));
    assert!(makefile.contains("make oauth-preflight"));
    assert!(makefile.contains("oauth-preflight: oauth-credentials"));
    assert!(makefile.contains("oauth-evidence:"));
    assert!(makefile.contains("oauth-callback: oauth-credentials"));
    assert!(makefile.contains("oauth-browser-smoke: oauth-preflight"));
    assert!(makefile.contains("oauth-dev-browser-smoke:"));
    assert!(!makefile.contains("placeholder"));

    let credential_script =
        std::fs::read_to_string(project.join("scripts/verify_oauth_credentials.sh")).unwrap();
    assert!(credential_script.contains("must start with https:// for live OAuth"));
    assert!(credential_script.contains("must use a provider-reachable host"));
    assert!(credential_script.contains("AUTH_${prefix}_REDIRECT_URI"));

    let spin_toml = std::fs::read_to_string(project.join("spin.toml")).unwrap();
    assert!(spin_toml.contains("database_backend = { default = \"sqlite\" }"));
    assert!(spin_toml.contains("auth_cookie_secure = { default = \"false\" }"));
    assert!(spin_toml.contains("auth_cookie_secure = \"{{ auth_cookie_secure }}\""));
    assert!(spin_toml.contains("auth_jwt_key_ring_json = \"{{ auth_jwt_key_ring_json }}\""));
    assert!(spin_toml.contains("auth_public_base_url = \"{{ auth_public_base_url }}\""));
    assert!(spin_toml.contains("auth_google_client_secret = \"{{ auth_google_client_secret }}\""));
    assert!(spin_toml.contains("auth_apple_private_key = \"{{ auth_apple_private_key }}\""));
    assert!(spin_toml.contains("database_backend = \"{{ database_backend }}\""));
    assert!(!spin_toml.contains("DATABASE_BACKEND = \"{{ database_backend }}\""));
    assert!(spin_toml.contains("postgres://*:*"));
    assert!(spin_toml.contains("mysql://*:*"));
    assert!(spin_toml.contains("${AUTH_STACK_FEATURES:-ssr,sqlite,spin-grpc}"));

    let production_spin_toml =
        std::fs::read_to_string(project.join("spin.production.toml.example")).unwrap();
    assert!(production_spin_toml.contains("auth_production_mode = { default = \"true\" }"));
    assert!(production_spin_toml.contains("auth_cookie_secure = { default = \"true\" }"));
    assert!(production_spin_toml.contains("auth_cookie_secure = \"{{ auth_cookie_secure }}\""));
    assert!(
        production_spin_toml.contains("auth_jwt_key_ring_json = \"{{ auth_jwt_key_ring_json }}\"")
    );
    assert!(production_spin_toml.contains("auth_public_base_url = \"{{ auth_public_base_url }}\""));
    assert!(production_spin_toml
        .contains("auth_google_client_secret = \"{{ auth_google_client_secret }}\""));
    assert!(
        production_spin_toml.contains("auth_apple_private_key = \"{{ auth_apple_private_key }}\"")
    );
    assert!(production_spin_toml.contains("https://auth.example.com"));
    assert!(production_spin_toml.contains("postgres://auth-db.internal.example.com:5432"));
    assert!(production_spin_toml.contains("mysql://auth-db.internal.example.com:3306"));
    assert!(!production_spin_toml.contains("*://"));
    assert!(!production_spin_toml.contains("://*"));
    assert!(!production_spin_toml.contains("*:*"));
    assert!(!production_spin_toml.contains("localhost"));
    assert!(!production_spin_toml.contains("127.0.0.1"));

    let env_example = std::fs::read_to_string(project.join(".env.example")).unwrap();
    assert!(env_example.contains("AUTH_STORAGE_AUTO_CATCH_UP=true"));
    assert!(env_example.contains("AUTH_COOKIE_SECURE=false"));
    assert!(env_example.contains("DATABASE_BACKEND=sqlite"));
    assert!(env_example.contains("POSTGRES_URL="));
    assert!(env_example.contains("MYSQL_URL="));

    let mut check = Command::cargo_bin("ddd").unwrap();
    check.arg("--cwd").arg(&project).arg("check");
    check.assert().success();
}

#[test]
fn auth_stack_rejects_non_fullstack_shape() {
    let temp = tempfile::tempdir().unwrap();

    let mut command = Command::cargo_bin("ddd").unwrap();
    command
        .arg("--cwd")
        .arg(temp.path())
        .arg("--dry-run")
        .arg("init")
        .arg("auth-api-only")
        .arg("--preset")
        .arg("auth-stack")
        .arg("--transport")
        .arg("http");

    command
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires transport=both"));
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
fn auth_enable_commands_update_manifest_without_secrets() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("authable");
    let mut init = Command::cargo_bin("ddd").unwrap();
    init.arg("--cwd")
        .arg(temp.path())
        .arg("init")
        .arg("authable")
        .arg("--preset")
        .arg("leptos-wasi")
        .arg("--domain")
        .arg("Workspace");
    init.assert().success();

    for args in [
        &["enable", "auth"][..],
        &["enable", "authz"][..],
        &["enable", "passkeys"][..],
        &["enable", "oauth-provider", "google"][..],
    ] {
        let mut command = Command::cargo_bin("ddd").unwrap();
        command.arg("--cwd").arg(&project).args(args);
        command.assert().success();
    }

    let manifest = std::fs::read_to_string(project.join("ddd.toml")).unwrap();
    assert!(manifest.contains(r#""auth""#));
    assert!(manifest.contains(r#""authz""#));
    assert!(manifest.contains(r#""passkeys""#));
    assert!(manifest.contains(r#""oauth:google""#));
    assert!(manifest.contains("[[auth.providers]]"));
    assert!(manifest.contains(r#"enabled_env = "AUTH_GOOGLE_ENABLED""#));
    assert!(manifest.contains(r#"client_id_env = "AUTH_GOOGLE_CLIENT_ID""#));
    assert!(manifest.contains(r#"client_secret_env = "AUTH_GOOGLE_CLIENT_SECRET""#));
    assert!(!manifest.contains("client_secret ="));
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
