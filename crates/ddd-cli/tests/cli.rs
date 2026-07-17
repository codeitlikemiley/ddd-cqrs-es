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
    "fullstack",
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
        .stdout(predicate::str::contains("fullstack"))
        .stdout(predicate::str::contains("oauth:google"));
}

#[test]
fn fullstack_dry_run_json_lists_auth_template_operations() {
    let temp = tempfile::tempdir().unwrap();

    let mut command = Command::cargo_bin("ddd").unwrap();
    command
        .arg("--cwd")
        .arg(temp.path())
        .arg("--dry-run")
        .arg("--format")
        .arg("json")
        .arg("init")
        .arg("fullstack")
        .arg("--preset")
        .arg("fullstack");

    command
        .assert()
        .success()
        .stdout(predicate::str::contains("\"path\": \"spin.toml\""))
        .stdout(predicate::str::contains(
            "\"path\": \"spin.production.toml.example\"",
        ))
        .stdout(predicate::str::contains("\"path\": \"src/app/mod.rs\""))
        .stdout(predicate::str::contains("\"path\": \"src/oauth.rs\""))
        .stdout(predicate::str::contains("\"path\": \"src/rest.rs\""))
        .stdout(predicate::str::contains("\"path\": \"src/grpc/mod.rs\""))
        .stdout(predicate::str::contains("\"path\": \"proto/auth.proto\""))
        .stdout(predicate::str::contains(
            "\"path\": \"proto/authorization.proto\"",
        ))
        .stdout(predicate::str::contains("\"path\": \".env.example\""));
}

#[test]
fn fullstack_writes_manifest_defaults_and_passes_check() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("fullstack");

    let mut init = Command::cargo_bin("ddd").unwrap();
    init.arg("--cwd")
        .arg(temp.path())
        .arg("init")
        .arg("fullstack")
        .arg("--preset")
        .arg("fullstack");
    init.assert().success();

    for file in [
        "ddd.toml",
        ".cargo/config.toml",
        "Cargo.toml",
        "Makefile",
        "build.rs",
        "input.css",
        "package.json",
        "package-lock.json",
        "spin.toml",
        "spin.production.toml.example",
        ".env.example",
        "src/app/mod.rs",
        "src/application/mod.rs",
        "src/contracts/mod.rs",
        "src/error.rs",
        "src/oauth.rs",
        "src/rest.rs",
        "src/grpc/mod.rs",
        "src/lib.rs",
        "src/main.rs",
        "src/server.rs",
        "src/store/mod.rs",
        "src/wasip3_random.rs",
        "proto/admin.proto",
        "proto/audit.proto",
        "proto/auth.proto",
        "proto/authorization.proto",
        "proto/organization.proto",
        "migrations/postgres/0001_app_storage.sql",
        "scripts/benchmark_fullstack.sh",
        "scripts/benchmark_ingress_overhead.sh",
        "scripts/soak_fullstack.sh",
        "scripts/report_oauth_evidence.sh",
        "scripts/reset_db.sh",
        "scripts/verify_auth_oauth_dev_browser.mjs",
        "scripts/verify_auth_pages.mjs",
        "scripts/verify_auth_passkeys.mjs",
        "scripts/verify_fullstack.sh",
        "scripts/verify_live_oauth_browser.mjs",
        "scripts/verify_live_oauth_callback.sh",
        "scripts/verify_live_oauth_preflight.sh",
        "scripts/verify_oauth_credentials.sh",
    ] {
        assert!(project.join(file).exists(), "{file} should be generated");
    }
    assert!(
        !project.join("src/domain/mod.rs").exists(),
        "fullstack should not generate an unused aggregate domain scaffold"
    );

    let manifest = std::fs::read_to_string(project.join("ddd.toml")).unwrap();
    assert!(manifest.contains(r#"preset = "fullstack""#));
    assert!(manifest.contains(r#"transport = "both""#));
    assert!(manifest.contains(r#"ui = "leptos""#));
    assert!(manifest.contains(r#""auth""#));
    assert!(manifest.contains(r#""authorization""#));
    assert!(manifest.contains("[auth]"));
    assert!(manifest.contains("[authorization]"));
    assert!(manifest.contains(r#"provider = "embedded-cedar""#));
    assert!(manifest.contains(r#"policy_revision = "embedded-v1""#));
    assert!(manifest.contains(r#"default_decision = "deny""#));

    let cargo_toml = std::fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(cargo_toml.contains("leptos_wasi"));
    assert!(cargo_toml.contains(r#"package = "leptos-wasi-runtime""#));
    assert!(cargo_toml.contains("spin-sdk"));
    assert!(cargo_toml.contains("wasi-auth"));
    assert!(!cargo_toml.contains("ddd-auth ="));
    assert!(!cargo_toml.contains("ddd-authz ="));
    assert!(cargo_toml.contains("=0.4.2-rc.1"));
    assert!(cargo_toml.contains("=0.1.0-rc.2")); // wasi-auth (independent cadence)
    assert!(cargo_toml.contains(&format!("={}", env!("CARGO_PKG_VERSION")))); // ddd_cqrs_es
    assert!(cargo_toml.contains("=0.7.0"));
    assert!(cargo_toml.contains("=0.57.1"));
    assert!(!cargo_toml.contains("wit-bindgen-spin-compat"));
    assert!(cargo_toml.contains("a02d330fe9357be2d18e6deef400511195ce6f7f"));
    assert!(cargo_toml.contains(r#"rust-version = "1.93.0""#));
    assert!(!cargo_toml.contains("path ="));
    assert!(cargo_toml.contains("[patch.crates-io]"));
    assert!(cargo_toml.contains(
        r#"spin-sdk = { git = "https://github.com/codeitlikemiley/spin-rust-sdk", rev = "a02d330fe9357be2d18e6deef400511195ce6f7f" }"#
    ));
    assert!(cargo_toml.contains("spin-postgres"));
    assert!(!cargo_toml.contains("spin-mysql"));
    assert!(!cargo_toml.contains("argon2 ="));
    assert!(!cargo_toml.contains("pbkdf2 ="));
    assert!(cargo_toml.contains("getrandom"));
    assert!(cargo_toml.contains(r#"bin-target-triple = "wasm32-wasip2""#));
    assert!(cargo_toml.contains(r#"mail-capture = ["wasi-auth/mail-capture"]"#));
    assert!(!cargo_toml.contains("mail-smtp"));
    assert!(cargo_toml.contains(r#"mail-http = ["wasi-auth/mail-http"]"#));
    assert!(cargo_toml.contains(r#"spicedb = ["wasi-auth/spicedb"]"#));
    assert!(!cargo_toml.contains(r#"features = ["fullstack-spin", "mail-capture"]"#));

    let makefile = std::fs::read_to_string(project.join("Makefile")).unwrap();
    assert!(makefile.contains("db ?= $(if $(DATABASE_BACKEND),$(DATABASE_BACKEND),postgres)"));
    assert!(!makefile.contains("AUTH_STORAGE_AUTO_CATCH_UP"));
    assert!(makefile.contains("AUTH_COOKIE_SECURE ?= false"));
    assert!(makefile.contains("--variable auth_cookie_secure=$(AUTH_COOKIE_SECURE)"));
    assert!(makefile.contains("--variable auth_jwt_key_ring_json='$(AUTH_JWT_KEY_RING_JSON)'"));
    assert!(makefile.contains("--variable auth_public_base_url=$(AUTH_PUBLIC_BASE_URL)"));
    assert!(makefile.contains("--variable auth_google_client_secret=$(AUTH_GOOGLE_CLIENT_SECRET)"));
    assert!(makefile.contains("--variable auth_apple_private_key='$(AUTH_APPLE_PRIVATE_KEY)'"));
    assert!(makefile.contains("POSTGRES_URL ?="));
    assert!(makefile.contains("FULLSTACK_FEATURES=$(GRPC_FEATURES)"));
    assert!(makefile.contains("MAIL_FEATURE := mail-http"));
    assert!(makefile.contains("MAIL_FEATURE := mail-capture"));
    assert!(makefile.contains("OPTIONAL_SPICEDB_FEATURE :="));
    assert!(makefile.contains("--variable auth_spicedb_enabled=$(AUTH_SPICEDB_ENABLED)"));
    assert!(makefile.contains("--variable auth_spicedb_check_token='$(AUTH_SPICEDB_CHECK_TOKEN)'"));
    assert!(!makefile.contains("--variable auth_spicedb_write_url"));
    assert!(!makefile.contains("--variable auth_mail_http_token"));
    assert!(makefile.contains("outbox-worker: validate-mail db-migrate install-outbox-worker"));
    assert!(makefile.contains("cargo install wasi-auth --version '=$(WASI_AUTH_VERSION)'"));
    assert!(makefile.contains("dev: install-outbox-worker"));
    assert!(makefile.contains("AUTH_RESEND_API_KEY='$(AUTH_RESEND_API_KEY)'"));
    assert!(makefile.contains("AUTH_RESEND_FROM_VALUE = $(subst"));
    assert!(makefile.contains("AUTH_RESEND_FROM='$(AUTH_RESEND_FROM_VALUE)'"));
    assert!(makefile.contains("\t@DATABASE_URL='$(POSTGRES_URL)' \\"));
    assert!(!makefile.contains("--variable auth_resend_api_key"));
    assert!(makefile.contains("WASI_AUTH_OUTBOX_WORKER_BIN"));
    assert!(makefile.contains("--target wasm32-wasip2"));
    assert!(makefile.contains("oauth-credentials:"));
    assert!(makefile.contains("bash scripts/verify_oauth_credentials.sh"));
    assert!(makefile.contains("bash scripts/verify_live_oauth_preflight.sh"));
    assert!(makefile.contains("bash scripts/report_oauth_evidence.sh"));
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
    assert!(spin_toml.contains("database_backend = { default = \"postgres\" }"));
    assert!(spin_toml.contains("auth_cookie_secure = { default = \"false\" }"));
    assert!(spin_toml.contains("auth_password_kdf = { default = \"argon2id\" }"));
    assert!(spin_toml.contains("auth_bootstrap_admin_emails = { default = \"\" }"));
    assert!(spin_toml.contains("auth_csrf_secret = { default = \"\" }"));
    assert!(spin_toml.contains("auth_dev_tools = { default = \"true\" }"));
    assert!(spin_toml.contains("auth_spicedb_enabled = { default = \"false\" }"));
    assert!(spin_toml.contains("auth_spicedb_check_token = { default = \"\" }"));
    assert!(!spin_toml.contains("auth_spicedb_write_url"));
    assert!(!spin_toml.contains("auth_spicedb_token"));
    assert!(!spin_toml.contains("auth_mail_http_token"));
    assert!(spin_toml.contains("auth_cookie_secure = \"{{ auth_cookie_secure }}\""));
    assert!(spin_toml.contains("auth_password_kdf = \"{{ auth_password_kdf }}\""));
    assert!(
        spin_toml.contains("auth_bootstrap_admin_emails = \"{{ auth_bootstrap_admin_emails }}\"")
    );
    assert!(spin_toml.contains("auth_csrf_secret = \"{{ auth_csrf_secret }}\""));
    assert!(spin_toml.contains("auth_dev_tools = \"{{ auth_dev_tools }}\""));
    assert!(spin_toml.contains("auth_jwt_key_ring_json = \"{{ auth_jwt_key_ring_json }}\""));
    assert!(spin_toml.contains("auth_public_base_url = \"{{ auth_public_base_url }}\""));
    assert!(spin_toml.contains("auth_google_client_secret = \"{{ auth_google_client_secret }}\""));
    assert!(spin_toml.contains("auth_apple_private_key = \"{{ auth_apple_private_key }}\""));
    assert!(spin_toml.contains("database_backend = \"{{ database_backend }}\""));
    assert!(!spin_toml.contains("DATABASE_BACKEND = \"{{ database_backend }}\""));
    assert!(spin_toml.contains("postgres://*:*"));
    assert!(!spin_toml.contains("mysql://*:*"));
    assert!(spin_toml.contains("${FULLSTACK_FEATURES:-ssr,postgres,spin-grpc,mail-capture}"));
    assert!(spin_toml.contains("target/wasm32-wasip2/release/fullstack.wasm"));

    let production_spin_toml =
        std::fs::read_to_string(project.join("spin.production.toml.example")).unwrap();
    assert!(production_spin_toml.contains("auth_production_mode = { default = \"true\" }"));
    assert!(production_spin_toml.contains("auth_cookie_secure = { default = \"true\" }"));
    assert!(production_spin_toml.contains("auth_password_kdf = { default = \"argon2id\" }"));
    assert!(production_spin_toml.contains("auth_bootstrap_admin_emails = { default = \"\" }"));
    assert!(production_spin_toml.contains("auth_cookie_secure = \"{{ auth_cookie_secure }}\""));
    assert!(production_spin_toml.contains("auth_password_kdf = \"{{ auth_password_kdf }}\""));
    assert!(production_spin_toml
        .contains("auth_bootstrap_admin_emails = \"{{ auth_bootstrap_admin_emails }}\""));
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
    assert!(!production_spin_toml.contains("*://"));
    assert!(!production_spin_toml.contains("://*"));
    assert!(!production_spin_toml.contains("*:*"));
    assert!(!production_spin_toml.contains("localhost"));
    assert!(!production_spin_toml.contains("127.0.0.1"));
    assert!(
        production_spin_toml.contains("${FULLSTACK_FEATURES:-ssr,postgres,spin-grpc,mail-http}")
    );
    assert!(production_spin_toml.contains("auth_mail_transport = { default = \"http\" }"));

    let env_example = std::fs::read_to_string(project.join(".env.example")).unwrap();
    assert!(!env_example.contains("AUTH_STORAGE_AUTO_CATCH_UP"));
    assert!(env_example.contains("AUTH_COOKIE_SECURE=false"));
    assert!(env_example.contains("AUTH_PASSWORD_KDF=argon2id"));
    assert!(env_example.contains("AUTH_BOOTSTRAP_ADMIN_EMAILS="));
    assert!(env_example.contains("AUTH_CSRF_SECRET="));
    assert!(env_example.contains("AUTH_DEV_TOOLS=true"));
    assert!(env_example.contains("AUTH_OUTBOX_KEY_BASE64="));
    assert!(env_example.contains("AUTH_OUTBOX_KEY_VERSION=development-v1"));
    assert!(env_example.contains("AUTH_SPICEDB_CHECK_TOKEN="));
    assert!(env_example.contains("DATABASE_BACKEND=postgres"));
    assert!(env_example.contains("POSTGRES_URL="));

    let compose = std::fs::read_to_string(project.join("compose.yaml")).unwrap();
    assert!(compose.contains("postgres:17-alpine"));
    assert!(compose.contains("54329:5432"));

    let mut check = Command::cargo_bin("ddd").unwrap();
    check.arg("--cwd").arg(&project).arg("check");
    check.assert().success();
}

#[test]
fn fullstack_rejects_non_fullstack_shape() {
    let temp = tempfile::tempdir().unwrap();

    let mut command = Command::cargo_bin("ddd").unwrap();
    command
        .arg("--cwd")
        .arg(temp.path())
        .arg("--dry-run")
        .arg("init")
        .arg("auth-api-only")
        .arg("--preset")
        .arg("fullstack")
        .arg("--transport")
        .arg("http");

    command
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires transport=both"));
}

fn init_fullstack_app(temp: &tempfile::TempDir, name: &str) {
    let mut init = Command::cargo_bin("ddd").unwrap();
    init.arg("--cwd")
        .arg(temp.path())
        .arg("init")
        .arg(name)
        .arg("--preset")
        .arg("fullstack");
    init.assert().success();
}

#[test]
fn fullstack_add_aggregate_bootstraps_product_domain() {
    let temp = tempfile::tempdir().unwrap();
    init_fullstack_app(&temp, "saas-add-agg");
    let project = temp.path().join("saas-add-agg");

    let mut command = Command::cargo_bin("ddd").unwrap();
    command
        .arg("--cwd")
        .arg(&project)
        .arg("add")
        .arg("aggregate")
        .arg("Billing");

    command.assert().success();

    assert!(project.join("src/domain/mod.rs").exists());
    assert!(project.join("src/domain/billing.rs").exists());
    assert!(project.join("tests/billing_domain.rs").exists());
    assert!(project.join("src/domain_app/mod.rs").exists());
    assert!(project.join("src/domain_app/billing.rs").exists());
    assert!(project.join("src/domain_rest.rs").exists());
    let lib = std::fs::read_to_string(project.join("src/lib.rs")).unwrap();
    assert!(
        lib.contains("pub mod domain;"),
        "lib.rs must register product domain"
    );
    assert!(
        lib.contains("mod domain_app;") && lib.contains("mod domain_rest;"),
        "lib.rs must register domain_app + domain_rest"
    );
    let rest = std::fs::read_to_string(project.join("src/rest.rs")).unwrap();
    assert!(
        rest.contains("/api/domain/") && rest.contains("domain_rest::dispatch"),
        "rest.rs must route /api/domain/*"
    );
    let domain_mod = std::fs::read_to_string(project.join("src/domain/mod.rs")).unwrap();
    assert!(domain_mod.contains("pub mod billing;"));
    assert!(domain_mod.contains("// ddd:domain-modules:end"));
    let aggregate = std::fs::read_to_string(project.join("src/domain/billing.rs")).unwrap();
    assert!(aggregate.contains("// ddd:events:end"));
    assert!(aggregate.contains("// ddd:commands:end"));
    let app = std::fs::read_to_string(project.join("src/domain_app/billing.rs")).unwrap();
    assert!(app.contains("InMemoryEventStore"));
    assert!(app.contains("execute_billing_command"));
    let domain_rest = std::fs::read_to_string(project.join("src/domain_rest.rs")).unwrap();
    assert!(domain_rest.contains("/api/domain/billing/"));
    let manifest = std::fs::read_to_string(project.join("ddd.toml")).unwrap();
    assert!(manifest.contains("Billing") || manifest.contains("billing"));

    let mut check = Command::cargo_bin("ddd").unwrap();
    check.arg("--cwd").arg(&project).arg("check");
    check.assert().success();
}

#[test]
fn fullstack_add_event_and_command_extend_domain() {
    let temp = tempfile::tempdir().unwrap();
    init_fullstack_app(&temp, "saas-add-event");
    let project = temp.path().join("saas-add-event");

    let mut add_agg = Command::cargo_bin("ddd").unwrap();
    add_agg
        .arg("--cwd")
        .arg(&project)
        .arg("add")
        .arg("aggregate")
        .arg("Invoice");
    add_agg.assert().success();

    let mut add_event = Command::cargo_bin("ddd").unwrap();
    add_event
        .arg("--cwd")
        .arg(&project)
        .arg("add")
        .arg("event")
        .arg("Invoice")
        .arg("Paid")
        .arg("--field")
        .arg("amount:i64");
    add_event.assert().success();

    let mut add_cmd = Command::cargo_bin("ddd").unwrap();
    add_cmd
        .arg("--cwd")
        .arg(&project)
        .arg("add")
        .arg("command")
        .arg("Invoice")
        .arg("PayInvoice")
        .arg("--field")
        .arg("amount:i64");
    add_cmd.assert().success();

    let aggregate = std::fs::read_to_string(project.join("src/domain/invoice.rs")).unwrap();
    assert!(aggregate.contains("Paid"));
    assert!(aggregate.contains("PayInvoice") || aggregate.contains("amount"));
}

#[test]
fn fullstack_rejects_orphan_projection_stub() {
    let temp = tempfile::tempdir().unwrap();
    init_fullstack_app(&temp, "saas-add-proj");
    let project = temp.path().join("saas-add-proj");

    let mut command = Command::cargo_bin("ddd").unwrap();
    command
        .arg("--cwd")
        .arg(&project)
        .arg("add")
        .arg("projection")
        .arg("Ledger");

    command
        .assert()
        .failure()
        .stderr(predicate::str::contains("preset=fullstack"))
        .stderr(predicate::str::contains("product-domain codegen only"));

    assert!(
        !project.join("src/projections/ledger.rs").exists(),
        "orphan projection stubs must not be written under fullstack"
    );
}

#[test]
fn fullstack_serve_plans_dev() {
    let temp = tempfile::tempdir().unwrap();
    init_fullstack_app(&temp, "saas-serve");
    let project = temp.path().join("saas-serve");

    let mut command = Command::cargo_bin("ddd").unwrap();
    command
        .arg("--cwd")
        .arg(&project)
        .arg("--dry-run")
        .arg("--format")
        .arg("json")
        .arg("serve");

    command
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dev\""))
        .stdout(predicate::str::contains("transport=both"))
        .stdout(predicate::str::contains("make"));
}

#[test]
fn fullstack_init_reports_next_steps() {
    let temp = tempfile::tempdir().unwrap();

    let mut command = Command::cargo_bin("ddd").unwrap();
    command
        .arg("--cwd")
        .arg(temp.path())
        .arg("--format")
        .arg("json")
        .arg("init")
        .arg("saas-next")
        .arg("--preset")
        .arg("fullstack");

    command
        .assert()
        .success()
        .stdout(predicate::str::contains("next_steps"))
        .stdout(predicate::str::contains("make db-up"))
        .stdout(predicate::str::contains("make dev transport=both"));
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
        &["enable", "authorization"][..],
        &["enable", "passkeys"][..],
        &["enable", "oauth-provider", "google"][..],
    ] {
        let mut command = Command::cargo_bin("ddd").unwrap();
        command.arg("--cwd").arg(&project).args(args);
        command.assert().success();
    }

    let manifest = std::fs::read_to_string(project.join("ddd.toml")).unwrap();
    assert!(manifest.contains(r#""auth""#));
    assert!(manifest.contains(r#""authorization""#));
    assert!(manifest.contains("[authorization]"));
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
