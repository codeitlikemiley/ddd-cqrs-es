mod manifest;
mod model;
mod operation;
mod render;

use crate::manifest::{DomainRecord, ProjectManifest, MANIFEST_FILE};
use crate::model::{
    defaults_for_preset, AppSelection, DbBackend, OAuthProviderKind, OutputFormat, Preset,
    Realtime, Runtime, Transport, Ui,
};
use crate::operation::{apply_operations, write_operation, CommandReport, FileOperation};
use crate::render::{
    available_template_names, parse_field_specs, render_command_handle_arm, render_command_variant,
    render_event_type_arm, render_event_variant, render_init, sanitize_package_name,
    InitRenderInput, NameParts,
};
use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use heck::{ToSnakeCase, ToUpperCamelCase};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml_edit::DocumentMut;

#[derive(Debug, Parser)]
#[command(name = "ddd", version, about = "Scaffold ddd_cqrs_es applications")]
pub struct Cli {
    #[arg(long, global = true)]
    cwd: Option<PathBuf>,
    #[arg(long, global = true)]
    dry_run: bool,
    #[arg(long, global = true)]
    yes: bool,
    #[arg(long, global = true)]
    force: bool,
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Init(InitArgs),
    Add(AddArgs),
    Enable(EnableArgs),
    Serve(RunArgs),
    Watch(RunArgs),
    Fresh(FreshArgs),
    Doctor,
    Check,
    Matrix,
    Capabilities(CapabilitiesArgs),
}

#[derive(Debug, Args)]
struct InitArgs {
    path: PathBuf,
    #[arg(long, value_enum, default_value_t = Preset::Basic)]
    preset: Preset,
    #[arg(long, value_enum)]
    runtime: Option<Runtime>,
    #[arg(long, value_enum)]
    db: Option<DbBackend>,
    #[arg(long, value_enum)]
    realtime: Option<Realtime>,
    #[arg(long, value_enum)]
    transport: Option<Transport>,
    #[arg(long, value_enum)]
    ui: Option<Ui>,
    #[arg(long, default_value = "Counter")]
    domain: String,
}

#[derive(Debug, Args)]
struct AddArgs {
    #[command(subcommand)]
    command: AddCommand,
}

#[derive(Debug, Subcommand)]
#[command(rename_all = "kebab-case")]
enum AddCommand {
    Aggregate(NamedAddArgs),
    Event(EventAddArgs),
    Command(CommandAddArgs),
    Error(NamedAddArgs),
    Projection(NamedAddArgs),
    Query(NamedAddArgs),
    ProcessManager(NamedAddArgs),
    Snapshot(NamedAddArgs),
    Upcaster(UpcasterAddArgs),
    Route(RouteAddArgs),
    GrpcMethod(NamedAddArgs),
    ServerFn(NamedAddArgs),
    RestEndpoint(RouteAddArgs),
    Test(NamedAddArgs),
}

#[derive(Debug, Args)]
struct NamedAddArgs {
    name: String,
}

#[derive(Debug, Args)]
struct EventAddArgs {
    aggregate: String,
    name: String,
    #[arg(long = "field")]
    fields: Vec<String>,
    #[arg(long)]
    event_type: Option<String>,
}

#[derive(Debug, Args)]
struct CommandAddArgs {
    aggregate: String,
    name: String,
    #[arg(long = "field")]
    fields: Vec<String>,
}

#[derive(Debug, Args)]
struct UpcasterAddArgs {
    event: String,
    #[arg(long, default_value_t = 1)]
    from: u32,
    #[arg(long, default_value_t = 2)]
    to: u32,
}

#[derive(Debug, Args)]
struct RouteAddArgs {
    name: String,
    #[arg(long, default_value = "GET")]
    method: String,
    #[arg(long)]
    path: Option<String>,
}

#[derive(Debug, Args)]
struct EnableArgs {
    #[command(subcommand)]
    command: EnableCommand,
}

#[derive(Debug, Subcommand)]
#[command(rename_all = "kebab-case")]
enum EnableCommand {
    Db {
        backend: DbBackend,
    },
    RedisStore,
    Realtime {
        mode: Realtime,
    },
    Grpc,
    Rest,
    Leptos,
    Auth,
    Authz,
    Passkeys,
    #[command(name = "oauth-provider")]
    OAuthProvider {
        provider: OAuthProviderKind,
    },
    Idempotency,
    Snapshots,
    Tracing,
}

#[derive(Debug, Args)]
struct RunArgs {
    #[arg(long, value_enum)]
    runtime: Option<Runtime>,
    #[arg(long, value_enum)]
    db: Option<DbBackend>,
    #[arg(long, value_enum)]
    realtime: Option<Realtime>,
    #[arg(long, value_enum)]
    transport: Option<Transport>,
}

#[derive(Debug, Args)]
struct FreshArgs {
    #[arg(long, value_enum)]
    db: Option<DbBackend>,
}

#[derive(Debug, Args)]
struct CapabilitiesArgs {
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Debug)]
struct ExecutionContext {
    cwd: PathBuf,
    dry_run: bool,
    force: bool,
}

pub fn run_from_env() -> Result<()> {
    let cli = Cli::parse();
    let force_json = matches!(&cli.command, Commands::Capabilities(args) if args.json);
    let format = cli.format;
    let report = execute(cli)?;
    print_report(format, force_json, &report)
}

pub fn execute(cli: Cli) -> Result<CommandReport> {
    let _yes = cli.yes;
    let ctx = ExecutionContext {
        cwd: cli
            .cwd
            .map(Ok)
            .unwrap_or_else(std::env::current_dir)
            .context("failed to resolve current directory")?,
        dry_run: cli.dry_run,
        force: cli.force,
    };

    match cli.command {
        Commands::Init(args) => init_project(&ctx, args),
        Commands::Add(args) => add_to_project(&ctx, args.command),
        Commands::Enable(args) => enable_capability(&ctx, args.command),
        Commands::Serve(args) => run_project(&ctx, args, RunMode::Serve),
        Commands::Watch(args) => run_project(&ctx, args, RunMode::Watch),
        Commands::Fresh(args) => fresh_project(&ctx, args),
        Commands::Doctor => doctor(&ctx),
        Commands::Check => check_project(&ctx),
        Commands::Matrix => matrix(),
        Commands::Capabilities(_) => capabilities(),
    }
}

fn init_project(ctx: &ExecutionContext, args: InitArgs) -> Result<CommandReport> {
    let (default_runtime, default_db, default_realtime, default_transport, default_ui) =
        defaults_for_preset(args.preset);
    let selection = AppSelection {
        preset: args.preset,
        runtime: args.runtime.unwrap_or(default_runtime),
        db: args.db.unwrap_or(default_db),
        realtime: args.realtime.unwrap_or(default_realtime),
        transport: args.transport.unwrap_or(default_transport),
        ui: args.ui.unwrap_or(default_ui),
    };
    selection.validate()?;

    let target = resolve_path(&ctx.cwd, &args.path);
    let package_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .map(sanitize_package_name)
        .unwrap_or_else(|| "ddd-app".to_string());
    let input = InitRenderInput {
        package_name,
        domain_name: args.domain,
        selection,
    };

    let operations = render_init(&input);
    let reports = apply_operations(&target, &operations, ctx.dry_run, ctx.force)?;
    let status = if ctx.dry_run { "planned" } else { "applied" };
    Ok(CommandReport::new(
        status,
        format!(
            "{} project `{}` at {}",
            status,
            input.package_name,
            target.display()
        ),
    )
    .with_operations(reports))
}

fn add_to_project(ctx: &ExecutionContext, command: AddCommand) -> Result<CommandReport> {
    let mut manifest = ProjectManifest::read_from(&ctx.cwd)?;
    let mut operations = Vec::new();

    match command {
        AddCommand::Aggregate(args) => {
            let names = NameParts::new(&args.name);
            manifest.add_domain(names.domain_record());
            let aggregate_path = format!("src/domain/{}.rs", names.module);
            operations.push(write_operation(
                aggregate_path.clone(),
                crate::render::render_init(&InitRenderInput {
                    package_name: manifest.name.clone(),
                    domain_name: args.name,
                    selection: manifest.selection(),
                })
                .into_iter()
                .find(|operation| operation.path == Path::new(&aggregate_path))
                .map(|operation| operation.content)
                .ok_or_else(|| anyhow::anyhow!("failed to render aggregate"))?,
                false,
                "aggregate module",
            ));
            let mod_path = PathBuf::from("src/domain/mod.rs");
            let mod_content = read_project_file(&ctx.cwd, &mod_path)?;
            let mod_content = insert_before_marker(
                &mod_content,
                "// ddd:domain-modules:end",
                &format!("pub mod {};\n", names.module),
            )?;
            let mod_content = insert_before_marker(
                &mod_content,
                "// ddd:domain-exports:end",
                &format!(
                    "pub use {}::{{{}, {}, {}, {}}};\n",
                    names.module,
                    names.aggregate,
                    names.command_type,
                    names.event_type,
                    names.id_type
                ),
            )?;
            operations.push(write_operation(
                mod_path,
                mod_content,
                true,
                "register and export aggregate module",
            ));
        }
        AddCommand::Event(args) => {
            let module = resolve_domain_module(&manifest, &args.aggregate)?;
            let domain = manifest
                .domains
                .iter()
                .find(|domain| domain.module == module)
                .context("domain not found")?;
            let fields = parse_field_specs(&args.fields)?;
            let variant = args.name.to_upper_camel_case();
            let event_type = args.event_type.unwrap_or_else(|| variant.to_snake_case());
            let path = format!("src/domain/{module}.rs");
            let relative_path = PathBuf::from(&path);
            let content = read_project_file(&ctx.cwd, &relative_path)?;
            let content = insert_before_marker(
                &content,
                "    // ddd:events:end",
                &render_event_variant(&variant, &fields),
            )?;
            let content = insert_before_marker(
                &content,
                "            // ddd:event-types:end",
                &render_event_type_arm(&event_type, &variant),
            )?;
            let content = insert_before_marker(
                &content,
                "            // ddd:apply-events:end",
                &format!(
                    "            {}::{} {{ .. }} => {{}}\n",
                    domain.event_type_name(),
                    variant
                ),
            )?;
            operations.push(write_operation(
                relative_path,
                content,
                true,
                "add domain event",
            ));
            manifest.add_event(&module, &variant);
        }
        AddCommand::Command(args) => {
            let module = resolve_domain_module(&manifest, &args.aggregate)?;
            let domain = manifest
                .domains
                .iter()
                .find(|domain| domain.module == module)
                .context("domain not found")?;
            let fields = parse_field_specs(&args.fields)?;
            let variant = args.name.to_upper_camel_case();
            let path = format!("src/domain/{module}.rs");
            let relative_path = PathBuf::from(&path);
            let content = read_project_file(&ctx.cwd, &relative_path)?;
            let content = insert_before_marker(
                &content,
                "    // ddd:commands:end",
                &render_command_variant(&variant, &fields),
            )?;
            let content = insert_before_marker(
                &content,
                "            // ddd:handle-commands:end",
                &render_command_handle_arm(&domain.command_type_name(), &variant),
            )?;
            operations.push(write_operation(relative_path, content, true, "add command"));
            manifest.add_command(&module, &variant);
        }
        AddCommand::Error(args) => operations.push(stub_operation(
            format!("src/errors/{}.rs", args.name.to_snake_case()),
            &args.name,
            "error type",
        )),
        AddCommand::Projection(args) => operations.push(stub_operation(
            format!("src/projections/{}.rs", args.name.to_snake_case()),
            &args.name,
            "projection",
        )),
        AddCommand::Query(args) => operations.push(stub_operation(
            format!("src/queries/{}.rs", args.name.to_snake_case()),
            &args.name,
            "query handler",
        )),
        AddCommand::ProcessManager(args) => operations.push(stub_operation(
            format!("src/process_managers/{}.rs", args.name.to_snake_case()),
            &args.name,
            "process manager",
        )),
        AddCommand::Snapshot(args) => operations.push(stub_operation(
            format!("src/snapshots/{}.rs", args.name.to_snake_case()),
            &args.name,
            "snapshot policy",
        )),
        AddCommand::Upcaster(args) => operations.push(write_operation(
            format!(
                "src/upcasters/{}_v{}_to_v{}.rs",
                args.event.to_snake_case(),
                args.from,
                args.to
            ),
            render_upcaster_stub(&args.event, args.from, args.to),
            false,
            "event upcaster",
        )),
        AddCommand::Route(args) | AddCommand::RestEndpoint(args) => {
            operations.push(write_operation(
                format!("src/routes/{}.rs", args.name.to_snake_case()),
                render_route_stub(&args.name, &args.method, args.path.as_deref()),
                false,
                "route scaffold",
            ))
        }
        AddCommand::GrpcMethod(args) => operations.push(stub_operation(
            format!("src/grpc/{}.rs", args.name.to_snake_case()),
            &args.name,
            "gRPC method",
        )),
        AddCommand::ServerFn(args) => operations.push(stub_operation(
            format!("src/server_functions/{}.rs", args.name.to_snake_case()),
            &args.name,
            "Leptos server function",
        )),
        AddCommand::Test(args) => operations.push(write_operation(
            format!("tests/{}_test.rs", args.name.to_snake_case()),
            format!(
                "#[test]\nfn {}_scenario() {{\n    // Arrange, act, assert.\n}}\n",
                args.name.to_snake_case()
            ),
            false,
            "test scaffold",
        )),
    }

    operations.push(write_operation(
        MANIFEST_FILE,
        manifest.to_toml(),
        true,
        "update project manifest",
    ));
    let reports = apply_operations(&ctx.cwd, &operations, ctx.dry_run, ctx.force)?;
    let status = if ctx.dry_run { "planned" } else { "applied" };
    Ok(CommandReport::new(status, "project extension complete").with_operations(reports))
}

fn enable_capability(ctx: &ExecutionContext, command: EnableCommand) -> Result<CommandReport> {
    let mut manifest = ProjectManifest::read_from(&ctx.cwd)?;
    let mut cargo_features = Vec::new();

    match command {
        EnableCommand::Db { backend } => {
            manifest.set_db(backend);
            cargo_features.push(backend.feature(manifest.runtime).to_string());
        }
        EnableCommand::RedisStore => {
            manifest.set_db(DbBackend::Redis);
            cargo_features.push(DbBackend::Redis.feature(manifest.runtime).to_string());
        }
        EnableCommand::Realtime { mode } => {
            manifest.set_realtime(mode);
            if mode == Realtime::Redis {
                cargo_features.push(DbBackend::Redis.feature(manifest.runtime).to_string());
            }
        }
        EnableCommand::Grpc => {
            if manifest.runtime != Runtime::Spin {
                anyhow::bail!("gRPC transport is Spin-only; set runtime=spin before enabling grpc");
            }
            manifest.set_transport(Transport::Both);
            cargo_features.push("spin-grpc".to_string());
        }
        EnableCommand::Rest => manifest.add_capability("rest"),
        EnableCommand::Leptos => {
            manifest.ui = Ui::Leptos;
            manifest.add_capability("leptos");
        }
        EnableCommand::Auth => manifest.enable_auth(),
        EnableCommand::Authz => manifest.enable_authz(),
        EnableCommand::Passkeys => manifest.enable_passkeys(),
        EnableCommand::OAuthProvider { provider } => manifest.enable_oauth_provider(provider),
        EnableCommand::Idempotency => manifest.add_capability("idempotency"),
        EnableCommand::Snapshots => manifest.add_capability("snapshots"),
        EnableCommand::Tracing => {
            manifest.add_capability("tracing");
            cargo_features.push("tracing".to_string());
        }
    }

    manifest.selection().validate()?;
    let mut operations = vec![write_operation(
        MANIFEST_FILE,
        manifest.to_toml(),
        true,
        "update project manifest",
    )];
    if !cargo_features.is_empty() {
        operations.push(write_operation(
            "Cargo.toml",
            patch_cargo_features(&ctx.cwd.join("Cargo.toml"), &cargo_features)?,
            true,
            "update ddd_cqrs_es features",
        ));
    }

    let reports = apply_operations(&ctx.cwd, &operations, ctx.dry_run, ctx.force)?;
    let status = if ctx.dry_run { "planned" } else { "applied" };
    Ok(CommandReport::new(status, "capability update complete").with_operations(reports))
}

#[derive(Clone, Copy)]
enum RunMode {
    Serve,
    Watch,
}

fn run_project(ctx: &ExecutionContext, args: RunArgs, mode: RunMode) -> Result<CommandReport> {
    let manifest = ProjectManifest::read_from(&ctx.cwd).ok();
    let runtime = args
        .runtime
        .or_else(|| manifest.as_ref().map(|manifest| manifest.runtime))
        .unwrap_or(Runtime::Spin);
    let db = args
        .db
        .or_else(|| manifest.as_ref().map(|manifest| manifest.db))
        .unwrap_or(DbBackend::Sqlite);
    let realtime = args
        .realtime
        .or_else(|| manifest.as_ref().map(|manifest| manifest.realtime))
        .unwrap_or(Realtime::Off);
    let transport = args
        .transport
        .or_else(|| manifest.as_ref().map(|manifest| manifest.transport))
        .unwrap_or(Transport::Http);
    AppSelection {
        preset: manifest
            .as_ref()
            .map(|manifest| manifest.preset)
            .unwrap_or(Preset::LeptosWasi),
        runtime,
        db,
        realtime,
        transport,
        ui: manifest
            .as_ref()
            .map(|manifest| manifest.ui)
            .unwrap_or(Ui::Leptos),
    }
    .validate()?;

    let command = match mode {
        RunMode::Serve => serve_command(runtime, db, realtime, transport),
        RunMode::Watch => watch_command(runtime, db, realtime, transport),
    };
    if !ctx.dry_run {
        run_external_command(&ctx.cwd, &command)?;
    }
    Ok(CommandReport::new(
        if ctx.dry_run { "planned" } else { "ok" },
        "runtime command resolved",
    )
    .with_command(command))
}

fn fresh_project(ctx: &ExecutionContext, args: FreshArgs) -> Result<CommandReport> {
    let manifest = ProjectManifest::read_from(&ctx.cwd).ok();
    let db = args
        .db
        .or_else(|| manifest.as_ref().map(|manifest| manifest.db))
        .unwrap_or(DbBackend::Sqlite);
    let command = vec!["make".to_string(), format!("db={db}"), "fresh".to_string()];
    if !ctx.dry_run {
        run_external_command(&ctx.cwd, &command)?;
    }
    Ok(CommandReport::new(
        if ctx.dry_run { "planned" } else { "ok" },
        "fresh reset command resolved",
    )
    .with_command(command))
}

fn doctor(_ctx: &ExecutionContext) -> Result<CommandReport> {
    let tools = [
        "cargo",
        "rustup",
        "rustfmt",
        "clippy-driver",
        "make",
        "spin",
    ];
    let results = tools
        .iter()
        .map(|tool| {
            let found = Command::new("sh")
                .arg("-c")
                .arg(format!("command -v {tool} >/dev/null 2>&1"))
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
            json!({ "tool": tool, "found": found })
        })
        .collect::<Vec<_>>();
    Ok(CommandReport::new("ok", "doctor completed").with_data(json!({ "tools": results })))
}

fn check_project(ctx: &ExecutionContext) -> Result<CommandReport> {
    let manifest = ProjectManifest::read_from(&ctx.cwd)?;
    manifest.selection().validate()?;
    let base_files = [MANIFEST_FILE, "Cargo.toml", "src/domain/mod.rs"];
    let auth_stack_files = [
        ".env.example",
        "build.rs",
        "input.css",
        "Makefile",
        "package.json",
        "spin.toml",
        "spin.production.toml.example",
        "src/app.rs",
        "src/application.rs",
        "src/contracts.rs",
        "src/error.rs",
        "src/grpc.rs",
        "src/lib.rs",
        "src/main.rs",
        "src/oauth.rs",
        "src/rest.rs",
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
    ];
    let files = if manifest.preset == Preset::AuthStack {
        [MANIFEST_FILE, "Cargo.toml"]
            .into_iter()
            .chain(auth_stack_files)
            .collect::<Vec<_>>()
    } else {
        base_files.into_iter().collect::<Vec<_>>()
    };
    let missing = files
        .iter()
        .filter(|file| !ctx.cwd.join(file).exists())
        .map(|file| file.to_string())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        anyhow::bail!("missing generated project files: {}", missing.join(", "));
    }
    Ok(CommandReport::new(
        "ok",
        "project manifest and generated files are valid",
    ))
}

fn matrix() -> Result<CommandReport> {
    let mut rows = Vec::new();
    for db in DbBackend::ALL {
        for realtime in Realtime::ALL {
            for transport in Transport::ALL {
                rows.push(json!({
                    "runtime": Runtime::Spin.as_str(),
                    "db": db.as_str(),
                    "realtime": realtime.as_str(),
                    "transport": transport.as_str(),
                    "redis_store": db == DbBackend::Redis,
                    "redis_wake": realtime == Realtime::Redis
                }));
            }
        }
    }
    Ok(CommandReport::new("ok", "matrix resolved").with_data(json!({ "matrix": rows })))
}

fn capabilities() -> Result<CommandReport> {
    Ok(CommandReport::new("ok", "capabilities resolved").with_data(json!({
        "templates": available_template_names(),
        "presets": Preset::ALL.map(|value| value.as_str()),
        "runtimes": Runtime::ALL.map(|value| value.as_str()),
        "db_backends": DbBackend::ALL.map(|value| value.as_str()),
        "realtime": Realtime::ALL.map(|value| value.as_str()),
        "transports": Transport::ALL.map(|value| value.as_str()),
        "ui": Ui::ALL.map(|value| value.as_str()),
        "auth": {
            "capabilities": [
                "auth",
                "authz",
                "passkeys",
                "oauth:google",
                "oauth:apple",
                "oauth:facebook"
            ],
            "oauth_providers": OAuthProviderKind::ALL.map(|value| value.as_str()),
            "default_preset": "auth-stack",
            "default_transport": "both",
            "default_ui": "leptos"
        },
        "commands": [
            "init", "add", "enable", "serve", "watch", "fresh", "doctor", "check", "matrix", "capabilities"
        ],
        "agent_contract": {
            "dry_run": true,
            "json_format": true,
            "manifest": MANIFEST_FILE
        }
    })))
}

fn serve_command(
    runtime: Runtime,
    db: DbBackend,
    realtime: Realtime,
    transport: Transport,
) -> Vec<String> {
    vec![
        "make".to_string(),
        runtime.as_str().to_string(),
        format!("db={db}"),
        format!("realtime={realtime}"),
        format!("transport={transport}"),
    ]
}

fn watch_command(
    runtime: Runtime,
    db: DbBackend,
    realtime: Realtime,
    transport: Transport,
) -> Vec<String> {
    vec![
        "cargo".to_string(),
        "watch".to_string(),
        "-s".to_string(),
        format!(
            "make {} db={db} realtime={realtime} transport={transport}",
            runtime
        ),
    ]
}

fn run_external_command(cwd: &Path, command: &[String]) -> Result<()> {
    let Some((program, args)) = command.split_first() else {
        anyhow::bail!("empty command");
    };
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("failed to start `{}`", command.join(" ")))?;
    if !status.success() {
        anyhow::bail!("command `{}` exited with {status}", command.join(" "));
    }
    Ok(())
}

fn print_report(format: OutputFormat, force_json: bool, report: &CommandReport) -> Result<()> {
    if format == OutputFormat::Json || force_json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }

    println!("{}", report.message);
    for operation in &report.operations {
        println!(
            "  {} {} ({} bytes) - {}",
            operation.action, operation.path, operation.bytes, operation.description
        );
    }
    if let Some(command) = &report.command {
        println!("  command: {}", command.join(" "));
    }
    if let Some(data) = &report.data {
        println!("{}", serde_json::to_string_pretty(data)?);
    }
    Ok(())
}

fn resolve_path(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn resolve_domain_module(manifest: &ProjectManifest, input: &str) -> Result<String> {
    let normalized = input.to_snake_case();
    manifest
        .domains
        .iter()
        .find(|domain| {
            domain.module == normalized
                || domain.aggregate == input
                || domain.aggregate.to_snake_case() == normalized
        })
        .map(|domain| domain.module.clone())
        .ok_or_else(|| anyhow::anyhow!("unknown aggregate `{input}`"))
}

trait DomainRecordNames {
    fn command_type_name(&self) -> String;
    fn event_type_name(&self) -> String;
}

impl DomainRecordNames for DomainRecord {
    fn command_type_name(&self) -> String {
        format!("{}Command", self.aggregate)
    }

    fn event_type_name(&self) -> String {
        format!("{}Event", self.aggregate)
    }
}

fn read_project_file(root: &Path, path: &Path) -> Result<String> {
    let full_path = root.join(path);
    std::fs::read_to_string(&full_path)
        .with_context(|| format!("failed to read {}", full_path.display()))
}

fn insert_before_marker(content: &str, marker: &str, insertion: &str) -> Result<String> {
    if content.contains(insertion.trim()) {
        return Ok(content.to_string());
    }
    let Some(index) = content.find(marker) else {
        anyhow::bail!("marker `{marker}` not found");
    };
    let mut patched = String::with_capacity(content.len() + insertion.len());
    patched.push_str(&content[..index]);
    patched.push_str(insertion);
    patched.push_str(&content[index..]);
    Ok(patched)
}

fn stub_operation(path: impl Into<PathBuf>, name: &str, kind: &str) -> FileOperation {
    let type_name = name.to_upper_camel_case();
    write_operation(
        path,
        format!(
            "pub struct {type_name};\n\nimpl {type_name} {{\n    pub fn name(&self) -> &'static str {{\n        \"{}\"\n    }}\n}}\n",
            name.to_snake_case()
        ),
        false,
        kind,
    )
}

fn render_route_stub(name: &str, method: &str, path: Option<&str>) -> String {
    let const_name = format!("{}_PATH", name.to_snake_case().to_uppercase());
    let route_path = path
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("/api/{}", name.to_snake_case().replace('_', "-")));
    format!(
        "pub const {const_name}: &str = \"{route_path}\";\npub const METHOD: &str = \"{}\";\n",
        method.to_ascii_uppercase()
    )
}

fn render_upcaster_stub(event: &str, from: u32, to: u32) -> String {
    let type_name = format!("{}V{from}ToV{to}Upcaster", event.to_upper_camel_case());
    format!(
        "pub struct {type_name};\n\nimpl ddd_cqrs_es::EventUpcaster for {type_name} {{\n    type Error = String;\n\n    fn source_version(&self) -> u32 {{ {from} }}\n\n    fn target_version(&self) -> u32 {{ {to} }}\n\n    fn upcast(&self, raw_payload: Vec<u8>) -> Result<Vec<u8>, Self::Error> {{\n        Ok(raw_payload)\n    }}\n}}\n"
    )
}

fn patch_cargo_features(path: &Path, features: &[String]) -> Result<String> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    text.parse::<DocumentMut>()
        .with_context(|| format!("{} is not valid TOML", path.display()))?;

    let mut patched = text;
    for feature in features {
        if patched.contains(&format!("\"{feature}\"")) {
            continue;
        }
        let marker = "features = [";
        let Some(index) = patched.find(marker) else {
            anyhow::bail!("Cargo.toml dependency ddd_cqrs_es must use a features array");
        };
        let insert_at = index + marker.len();
        patched.insert_str(insert_at, &format!("\"{feature}\", "));
    }

    patched
        .parse::<DocumentMut>()
        .with_context(|| "patched Cargo.toml is not valid TOML")?;
    Ok(patched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn spin_allows_grpc_transport() {
        let selection = AppSelection {
            preset: Preset::LeptosWasi,
            runtime: Runtime::Spin,
            db: DbBackend::Sqlite,
            realtime: Realtime::Off,
            transport: Transport::Grpc,
            ui: Ui::Leptos,
        };

        assert!(selection.validate().is_ok());
    }

    #[test]
    fn init_dry_run_reports_manifest_creation() {
        let tmp = tempfile::tempdir().unwrap();
        let cli = Cli::parse_from([
            "ddd",
            "--cwd",
            tmp.path().to_str().unwrap(),
            "--dry-run",
            "init",
            "sample-app",
        ]);

        let report = execute(cli).unwrap();

        assert!(report
            .operations
            .iter()
            .any(|operation| operation.path == MANIFEST_FILE));
    }

    #[test]
    fn matrix_contains_spin_grpc_combination() {
        let report = matrix().unwrap();
        let data = report.data.unwrap();
        let rows = data["matrix"].as_array().unwrap();

        assert!(rows.iter().any(|row| {
            row["runtime"] == "spin" && row["transport"] == "grpc" && row["db"] == "sqlite"
        }));
    }
}
