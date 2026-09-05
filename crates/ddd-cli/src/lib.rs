mod manifest;
mod model;
mod operation;
mod render;

use crate::manifest::{DomainRecord, ProjectManifest, MANIFEST_FILE};
use crate::model::{
    defaults_for_preset, AppSelection, DbBackend, OAuthProviderKind, OutputFormat, Preset,
    Realtime, Runtime, Transport, Ui,
};
use crate::operation::{
    apply_operations, contained_join, write_operation, CommandReport, FileOperation,
};
use crate::render::{
    available_template_names, ensure_event_type_name, ensure_package_name, ensure_rust_identifier,
    ensure_snake_identifier, parse_field_specs, render_aggregate, render_command_handle_arm,
    render_command_variant, render_domain_mod, render_domain_test, render_event_type_arm,
    render_event_variant, render_fullstack_domain_app_mod, render_fullstack_domain_app_module,
    render_fullstack_domain_rest_arm, render_fullstack_domain_rest_bootstrap, render_init,
    sanitize_package_name, InitRenderInput, NameParts,
};
use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use heck::{ToSnakeCase, ToUpperCamelCase};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml_edit::{DocumentMut, Item};

#[derive(Debug, Parser)]
#[command(name = "ddd", version, about = "Scaffold ddd_cqrs_es applications")]
pub struct Cli {
    #[arg(long, global = true)]
    cwd: Option<PathBuf>,
    #[arg(long, global = true)]
    dry_run: bool,
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
    #[command(alias = "authz")]
    Authorization,
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
    match execute(cli) {
        Ok(report) => print_report(format, force_json, &report),
        Err(error) => {
            if format == OutputFormat::Json || force_json {
                let envelope = json!({
                    "status": "error",
                    "message": error.to_string(),
                });
                println!("{}", serde_json::to_string_pretty(&envelope)?);
                std::process::exit(1);
            }
            Err(error)
        }
    }
}

pub fn execute(cli: Cli) -> Result<CommandReport> {
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
    if !ctx.dry_run && !ctx.force && target.exists() {
        let mut entries = std::fs::read_dir(&target)
            .with_context(|| format!("failed to inspect {}", target.display()))?;
        if entries.next().is_some() {
            anyhow::bail!(
                "target `{}` is not empty; choose an empty directory or rerun with --force",
                target.display()
            );
        }
    }
    let domain_names = NameParts::new(&args.domain);
    ensure_rust_identifier(&domain_names.aggregate, "domain name")?;
    ensure_snake_identifier(&domain_names.module, "domain module")?;
    let package_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .map(sanitize_package_name)
        .unwrap_or_else(|| "ddd-app".to_string());
    // Refuse now rather than writing a manifest that every later command
    // (which re-reads and re-validates `project.name`) would reject.
    ensure_package_name(&package_name, "project name")?;
    let input = InitRenderInput {
        package_name,
        domain_name: args.domain,
        selection,
    };

    let operations = render_init(&input);
    let reports = apply_operations(&target, &operations, ctx.dry_run, ctx.force)?;
    let status = if ctx.dry_run { "planned" } else { "applied" };
    let mut report = CommandReport::new(
        status,
        format!(
            "{} project `{}` at {}",
            status,
            input.package_name,
            target.display()
        ),
    )
    .with_operations(reports);

    if selection.preset == Preset::Fullstack {
        let dir_name = target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&input.package_name);
        report = report.with_data(json!({
            "preset": "fullstack",
            "next_steps": [
                format!("cd {dir_name}"),
                "make db-up",
                "make dev transport=both",
                "open http://localhost:3008  # or visit in a browser"
            ],
            "notes": [
                ".env is generated with a per-project AUTH_ROOT_KEY_BASE64; keep it out of version control",
                "make dev starts Spin plus the wasi-auth outbox worker (required for verification mail)",
                "ddd add aggregate wires src/domain + domain_app + /api/domain REST (InMemory demo store)",
                "replace InMemoryEventStore for production; domain routes are not Cedar-gated by default"
            ]
        }));
    }

    Ok(report)
}

fn fullstack_stub_unsupported_message() -> String {
    "preset=fullstack supports product-domain codegen only: \
     `ddd add aggregate|event|command` (and optional `ddd add test`). \
     Route/projection/server-fn/grpc stubs are not auto-wired into the product shell \
     (app/rest/grpc/store). Scaffold those by hand under `src/app`, `src/rest`, \
     `src/application`, or use `--preset basic` / `leptos-wasi` for thin apps. \
     Runtime: `ddd serve` → `make dev transport=both` after `make db-up`."
        .to_string()
}

fn refuse_fullstack_unwired_stub(manifest: &ProjectManifest, command: &AddCommand) -> Result<()> {
    if manifest.preset != Preset::Fullstack {
        return Ok(());
    }
    match command {
        AddCommand::Aggregate(_)
        | AddCommand::Event(_)
        | AddCommand::Command(_)
        | AddCommand::Test(_) => Ok(()),
        _ => anyhow::bail!("{}", fullstack_stub_unsupported_message()),
    }
}

/// Ensure `src/domain/mod.rs` exists (bootstraps on first aggregate for fullstack).
fn ensure_domain_mod_content(
    cwd: &Path,
    names: &NameParts,
    force_create: bool,
) -> Result<(String, bool)> {
    let mod_path = cwd.join("src/domain/mod.rs");
    if mod_path.exists() {
        let mut content = std::fs::read_to_string(&mod_path)
            .with_context(|| format!("failed to read {}", mod_path.display()))?;
        content = insert_before_marker(
            &content,
            "// ddd:domain-modules:end",
            &format!("pub mod {};\n", names.module),
        )?;
        content = insert_before_marker(
            &content,
            "// ddd:domain-exports:end",
            &format!(
                "pub use {}::{{{}, {}, {}, {}}};\n",
                names.module, names.aggregate, names.command_type, names.event_type, names.id_type
            ),
        )?;
        Ok((content, true))
    } else if force_create {
        Ok((render_domain_mod(names), false))
    } else {
        anyhow::bail!(
            "missing src/domain/mod.rs; run `ddd add aggregate <Name>` first to bootstrap the product domain"
        );
    }
}

/// Returns whether `src/lib.rs` already declares the product domain module.
fn lib_declares_domain_mod(content: &str) -> bool {
    content.lines().any(|line| {
        matches!(
            line.trim(),
            "mod domain;"
                | "pub mod domain;"
                | "#[cfg(feature = \"ssr\")] mod domain;"
                | "#[cfg(feature = \"ssr\")] pub mod domain;"
        )
    })
}

/// Register domain (+ domain_app / domain_rest) in fullstack `src/lib.rs`.
fn ensure_fullstack_lib_domain_modules(cwd: &Path) -> Result<Option<String>> {
    let lib_path = cwd.join("src/lib.rs");
    if !lib_path.exists() {
        return Ok(None);
    }
    let mut content = std::fs::read_to_string(&lib_path)
        .with_context(|| format!("failed to read {}", lib_path.display()))?;
    let mut changed = false;

    if !lib_declares_domain_mod(&content) {
        if content.contains("// ddd:product-domain:end") {
            content = insert_before_marker(
                &content,
                "// ddd:product-domain:end",
                "#[cfg(feature = \"ssr\")]\npub mod domain;\n",
            )?;
            changed = true;
        } else {
            anyhow::bail!(
                "could not register `pub mod domain` in src/lib.rs; add `// ddd:product-domain` markers"
            );
        }
    }

    if !content.contains("mod domain_app;") {
        if content.contains("// ddd:product-domain-app:end") {
            content = insert_before_marker(
                &content,
                "// ddd:product-domain-app:end",
                "#[cfg(feature = \"ssr\")]\nmod domain_app;\n#[cfg(feature = \"ssr\")]\nmod domain_rest;\n",
            )?;
            changed = true;
        } else {
            // Insert after product-domain block if present.
            let insertion = "\n// ddd:product-domain-app\n#[cfg(feature = \"ssr\")]\nmod domain_app;\n#[cfg(feature = \"ssr\")]\nmod domain_rest;\n// ddd:product-domain-app:end\n";
            if let Some(index) = content.find("// ddd:product-domain:end") {
                let insert_at = index + "// ddd:product-domain:end".len();
                let mut patched = String::new();
                patched.push_str(&content[..insert_at]);
                patched.push_str(insertion);
                patched.push_str(&content[insert_at..]);
                content = patched;
                changed = true;
            } else {
                anyhow::bail!(
                    "could not register domain_app/domain_rest in src/lib.rs; add product-domain-app markers"
                );
            }
        }
    }

    Ok(if changed { Some(content) } else { None })
}

fn ensure_fullstack_rest_domain_hooks(cwd: &Path) -> Result<Option<String>> {
    let rest_path = cwd.join("src/rest.rs");
    if !rest_path.exists() {
        return Ok(None);
    }
    let mut content = std::fs::read_to_string(&rest_path)
        .with_context(|| format!("failed to read {}", rest_path.display()))?;
    let mut changed = false;

    if !content.contains("/api/domain/") {
        if content.contains("// ddd:domain-rest-prefix:end") {
            content = insert_before_marker(
                &content,
                "// ddd:domain-rest-prefix:end",
                "        || path.starts_with(\"/api/domain/\")\n",
            )?;
            changed = true;
        } else if let Some(index) = content.find("path.starts_with(\"/api/audit/\")") {
            let insert_at = index + "path.starts_with(\"/api/audit/\")".len();
            let mut patched = String::new();
            patched.push_str(&content[..insert_at]);
            patched.push_str(
                "\n        // ddd:domain-rest-prefix\n        || path.starts_with(\"/api/domain/\")\n        // ddd:domain-rest-prefix:end",
            );
            patched.push_str(&content[insert_at..]);
            content = patched;
            changed = true;
        } else {
            anyhow::bail!(
                "could not wire /api/domain/ prefix in src/rest.rs; add domain-rest-prefix markers"
            );
        }
    }

    if !content.contains("domain_rest::dispatch") {
        let hook = "    if path.starts_with(\"/api/domain/\") {\n        return crate::domain_rest::dispatch(req).await;\n    }\n";
        if content.contains("// ddd:domain-rest-dispatch:end") {
            content = insert_before_marker(&content, "// ddd:domain-rest-dispatch:end", hook)?;
            changed = true;
        } else if let Some(index) =
            content.find("let cookie_session_id = cookie_session_id_from_request(&req);")
        {
            let mut patched = String::new();
            patched.push_str(&content[..index]);
            patched.push_str("// ddd:domain-rest-dispatch\n");
            patched.push_str(hook);
            patched.push_str("// ddd:domain-rest-dispatch:end\n    ");
            patched.push_str(&content[index..]);
            content = patched;
            changed = true;
        } else {
            anyhow::bail!(
                "could not wire domain REST dispatch in src/rest.rs; add domain-rest-dispatch markers"
            );
        }
    }

    Ok(if changed { Some(content) } else { None })
}

fn fullstack_product_domain_wiring(
    cwd: &Path,
    names: &NameParts,
    force: bool,
) -> Result<Vec<crate::operation::FileOperation>> {
    let mut operations = Vec::new();

    // domain_app module
    let app_mod_path = cwd.join("src/domain_app/mod.rs");
    if app_mod_path.exists() {
        let mut content = std::fs::read_to_string(&app_mod_path)
            .with_context(|| format!("failed to read {}", app_mod_path.display()))?;
        content = insert_before_marker(
            &content,
            "// ddd:domain-app-modules:end",
            &format!("pub mod {};\n", names.module),
        )?;
        content = insert_before_marker(
            &content,
            "// ddd:domain-app-exports:end",
            &format!("pub use {}::*;\n", names.module),
        )?;
        operations.push(write_operation(
            "src/domain_app/mod.rs",
            content,
            true,
            "register domain_app module",
        ));
    } else {
        operations.push(write_operation(
            "src/domain_app/mod.rs",
            render_fullstack_domain_app_mod(names),
            false,
            "bootstrap domain_app module",
        ));
    }
    operations.push(write_operation(
        format!("src/domain_app/{}.rs", names.module),
        render_fullstack_domain_app_module(names),
        force,
        "domain application service",
    ));

    // domain_rest router
    let rest_domain_path = cwd.join("src/domain_rest.rs");
    if rest_domain_path.exists() {
        let mut content = std::fs::read_to_string(&rest_domain_path)
            .with_context(|| format!("failed to read {}", rest_domain_path.display()))?;
        // Import new command/id types if missing.
        let use_line = format!(
            "use crate::domain::{{{}, {}}};\n",
            names.command_type, names.id_type
        );
        if !content.contains(&names.command_type) {
            if let Some(index) = content.find("use crate::domain_app;") {
                let mut patched = String::new();
                patched.push_str(&content[..index]);
                patched.push_str(&use_line);
                patched.push_str(&content[index..]);
                content = patched;
            }
        }
        content = insert_before_marker(
            &content,
            "        // ddd:domain-rest-arms:end",
            &render_fullstack_domain_rest_arm(names),
        )?;
        operations.push(write_operation(
            "src/domain_rest.rs",
            content,
            true,
            "extend domain REST dispatch",
        ));
    } else {
        operations.push(write_operation(
            "src/domain_rest.rs",
            render_fullstack_domain_rest_bootstrap(names),
            false,
            "bootstrap domain REST surface",
        ));
    }

    if let Some(lib_content) = ensure_fullstack_lib_domain_modules(cwd)? {
        operations.push(write_operation(
            "src/lib.rs",
            lib_content,
            true,
            "register domain modules in library root",
        ));
    }
    if let Some(rest_content) = ensure_fullstack_rest_domain_hooks(cwd)? {
        operations.push(write_operation(
            "src/rest.rs",
            rest_content,
            true,
            "hook domain REST into product rest router",
        ));
    }

    Ok(operations)
}

fn add_to_project(ctx: &ExecutionContext, command: AddCommand) -> Result<CommandReport> {
    let mut manifest = ProjectManifest::read_from(&ctx.cwd)?;
    refuse_fullstack_unwired_stub(&manifest, &command)?;
    validate_add_names(&command)?;
    let mut operations = Vec::new();

    match command {
        AddCommand::Aggregate(args) => {
            let names = NameParts::new(&args.name);
            ensure_rust_identifier(&names.aggregate, "aggregate name")?;
            ensure_snake_identifier(&names.module, "aggregate module")?;
            if ctx
                .cwd
                .join(format!("src/domain/{}.rs", names.module))
                .exists()
                && !ctx.force
            {
                anyhow::bail!(
                    "aggregate module src/domain/{}.rs already exists (use --force to overwrite)",
                    names.module
                );
            }
            manifest.add_domain(names.domain_record());
            let aggregate_path = format!("src/domain/{}.rs", names.module);
            operations.push(write_operation(
                aggregate_path,
                render_aggregate(&names),
                ctx.force,
                "aggregate module",
            ));
            let (mod_content, domain_mod_existed) =
                ensure_domain_mod_content(&ctx.cwd, &names, true)?;
            operations.push(write_operation(
                "src/domain/mod.rs",
                mod_content,
                domain_mod_existed,
                if domain_mod_existed {
                    "register and export aggregate module"
                } else {
                    "bootstrap product domain module"
                },
            ));
            operations.push(write_operation(
                format!("tests/{}_domain.rs", names.module),
                render_domain_test(
                    &InitRenderInput {
                        package_name: manifest.name.clone(),
                        domain_name: args.name.clone(),
                        selection: manifest.selection(),
                    },
                    &names,
                ),
                false,
                "aggregate fixture test",
            ));
            if manifest.preset == Preset::Fullstack {
                operations.extend(fullstack_product_domain_wiring(
                    &ctx.cwd, &names, ctx.force,
                )?);
            }
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
            ensure_rust_identifier(&variant, "event name")?;
            if domain.events.iter().any(|event| event == &variant) {
                anyhow::bail!(
                    "event `{variant}` already exists for aggregate `{}`",
                    domain.aggregate
                );
            }
            let event_type = args.event_type.unwrap_or_else(|| variant.to_snake_case());
            ensure_event_type_name(&event_type, "event type")?;
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
                &render_event_type_arm(&event_type, &variant)?,
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
            ensure_rust_identifier(&variant, "command name")?;
            if domain.commands.iter().any(|command| command == &variant) {
                anyhow::bail!(
                    "command `{variant}` already exists for aggregate `{}`",
                    domain.aggregate
                );
            }
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
                render_route_stub(&args.name, &args.method, args.path.as_deref())?,
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
        manifest_write_content(&ctx.cwd, &manifest)?,
        true,
        "update project manifest",
    ));
    let reports = apply_operations(&ctx.cwd, &operations, ctx.dry_run, ctx.force)?;
    let status = if ctx.dry_run { "planned" } else { "applied" };
    Ok(CommandReport::new(status, "project extension complete").with_operations(reports))
}

/// Validates user-supplied names before any codegen: the derived snake_case
/// form must be a safe Rust identifier and path segment.
fn validate_add_names(command: &AddCommand) -> Result<()> {
    fn check(name: &str, label: &str) -> Result<()> {
        ensure_snake_identifier(&name.to_snake_case(), label)
    }
    match command {
        AddCommand::Aggregate(args) => check(&args.name, "aggregate name"),
        AddCommand::Event(args) => {
            check(&args.aggregate, "aggregate name")?;
            check(&args.name, "event name")
        }
        AddCommand::Command(args) => {
            check(&args.aggregate, "aggregate name")?;
            check(&args.name, "command name")
        }
        AddCommand::Error(args) => check(&args.name, "error name"),
        AddCommand::Projection(args) => check(&args.name, "projection name"),
        AddCommand::Query(args) => check(&args.name, "query name"),
        AddCommand::ProcessManager(args) => check(&args.name, "process manager name"),
        AddCommand::Snapshot(args) => check(&args.name, "snapshot policy name"),
        AddCommand::Upcaster(args) => {
            check(&args.event, "upcaster event name")?;
            if args.from >= args.to {
                anyhow::bail!(
                    "upcaster source version ({}) must be less than target version ({})",
                    args.from,
                    args.to
                );
            }
            Ok(())
        }
        AddCommand::Route(args) | AddCommand::RestEndpoint(args) => check(&args.name, "route name"),
        AddCommand::GrpcMethod(args) => check(&args.name, "gRPC method name"),
        AddCommand::ServerFn(args) => check(&args.name, "server function name"),
        AddCommand::Test(args) => check(&args.name, "test name"),
    }
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
        EnableCommand::Authorization => manifest.enable_authorization(),
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
        manifest_write_content(&ctx.cwd, &manifest)?,
        true,
        "update project manifest",
    )];
    // Fullstack Cargo.toml is a large product manifest; the naive first-`features = [`
    // patch targets the wrong table. Feature wiring is already baked into the template.
    if !cargo_features.is_empty() && manifest.preset != Preset::Fullstack {
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
    let preset = manifest
        .as_ref()
        .map(|manifest| manifest.preset)
        .unwrap_or(Preset::LeptosWasi);
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
        preset,
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
        RunMode::Serve => serve_command(preset, runtime, db, realtime, transport),
        RunMode::Watch => watch_command(preset, runtime, db, realtime, transport),
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

fn tool_on_path(tool: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        let candidates = [
            dir.join(tool),
            #[cfg(windows)]
            dir.join(format!("{tool}.exe")),
            #[cfg(windows)]
            dir.join(format!("{tool}.cmd")),
            #[cfg(windows)]
            dir.join(format!("{tool}.bat")),
        ];
        candidates
            .iter()
            .any(|candidate| is_executable_file(candidate))
    })
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
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
        .map(|tool| json!({ "tool": tool, "found": tool_on_path(tool) }))
        .collect::<Vec<_>>();
    Ok(CommandReport::new("ok", "doctor completed").with_data(json!({ "tools": results })))
}

fn check_project(ctx: &ExecutionContext) -> Result<CommandReport> {
    let manifest = ProjectManifest::read_from(&ctx.cwd)?;
    manifest.selection().validate()?;
    let base_files = [MANIFEST_FILE, "Cargo.toml", "src/domain/mod.rs"];
    let fullstack_files = [
        ".cargo/config.toml",
        ".env.example",
        "build.rs",
        "compose.yaml",
        "input.css",
        "Makefile",
        "package.json",
        "spin.toml",
        "spin.production.toml.example",
        "src/app/mod.rs",
        "src/application/mod.rs",
        "src/auth_product/mod.rs",
        "src/bin/wasi-auth-migrate.rs",
        "src/contracts/mod.rs",
        "src/error.rs",
        "src/grpc/mod.rs",
        "src/lib.rs",
        "src/main.rs",
        "src/oauth.rs",
        "src/rest.rs",
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
    ];
    let files = if manifest.preset == Preset::Fullstack {
        [MANIFEST_FILE, "Cargo.toml"]
            .into_iter()
            .chain(fullstack_files)
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
                "authorization",
                "passkeys",
                "oauth:google",
                "oauth:apple",
                "oauth:facebook"
            ],
            "oauth_providers": OAuthProviderKind::ALL.map(|value| value.as_str()),
            "default_preset": "fullstack",
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
    preset: Preset,
    runtime: Runtime,
    db: DbBackend,
    realtime: Realtime,
    transport: Transport,
) -> Vec<String> {
    // Fullstack product Makefile: `dev` runs Spin + outbox worker (mail delivery).
    // `make spin` alone leaves verification email pending.
    if preset == Preset::Fullstack {
        return vec![
            "make".to_string(),
            "dev".to_string(),
            format!("transport={transport}"),
        ];
    }
    vec![
        "make".to_string(),
        runtime.as_str().to_string(),
        format!("db={db}"),
        format!("realtime={realtime}"),
        format!("transport={transport}"),
    ]
}

fn watch_command(
    preset: Preset,
    runtime: Runtime,
    db: DbBackend,
    realtime: Realtime,
    transport: Transport,
) -> Vec<String> {
    let make_script = if preset == Preset::Fullstack {
        format!("make dev transport={transport}")
    } else {
        format!(
            "make {} db={db} realtime={realtime} transport={transport}",
            runtime
        )
    };
    vec![
        "cargo".to_string(),
        "watch".to_string(),
        "-s".to_string(),
        make_script,
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
        write_stdout_line(&serde_json::to_string_pretty(report)?)?;
        return Ok(());
    }

    write_stdout_line(&report.message)?;
    for operation in &report.operations {
        write_stdout_line(&format!(
            "  {} {} ({} bytes) - {}",
            operation.action, operation.path, operation.bytes, operation.description
        ))?;
    }
    if let Some(command) = &report.command {
        write_stdout_line(&format!("  command: {}", command.join(" ")))?;
    }
    if let Some(data) = &report.data {
        write_stdout_line(&serde_json::to_string_pretty(data)?)?;
    }
    Ok(())
}

fn write_stdout_line(line: &str) -> Result<()> {
    use std::io::{self, Write};

    if let Err(error) = writeln!(io::stdout(), "{line}") {
        if error.kind() == io::ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
        return Err(error.into());
    }
    Ok(())
}

fn manifest_write_content(cwd: &Path, manifest: &ProjectManifest) -> Result<String> {
    let path = ProjectManifest::manifest_path(cwd);
    let existing = path
        .exists()
        .then(|| std::fs::read_to_string(&path))
        .transpose()
        .with_context(|| format!("failed to read {}", path.display()))?;
    manifest.to_toml_preserving(existing.as_deref())
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
    let full_path = contained_join(root, path)?;
    std::fs::read_to_string(&full_path)
        .with_context(|| format!("failed to read {}", full_path.display()))
}

fn marker_block<'a>(content: &'a str, end_marker: &str) -> Option<&'a str> {
    let end = find_unique_marker_line(content, end_marker).ok()?;
    let start_marker = end_marker.trim().strip_suffix(":end")?;
    let prefix = &content[..end];
    let start = find_last_unique_marker_line(prefix, start_marker).ok()?;
    Some(&content[start..end])
}

fn block_contains_trimmed_lines(block: &str, insertion: &str) -> bool {
    let needles: Vec<&str> = insertion
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if needles.is_empty() {
        return block.contains(insertion.trim());
    }
    needles
        .iter()
        .all(|needle| block.lines().any(|line| line.trim() == *needle))
}

fn find_unique_marker_line(content: &str, marker: &str) -> Result<usize> {
    let marker = marker.trim();
    let mut found = None;
    let mut offset = 0;
    for line in content.split_inclusive('\n') {
        let line_body = line.strip_suffix('\n').unwrap_or(line);
        if line_body.trim() == marker {
            if found.is_some() {
                anyhow::bail!("marker `{marker}` appears more than once");
            }
            found = Some(offset);
        }
        offset += line.len();
    }
    found.ok_or_else(|| anyhow::anyhow!("marker `{marker}` not found"))
}

fn find_last_unique_marker_line(content: &str, marker: &str) -> Result<usize> {
    let marker = marker.trim();
    let mut found = None;
    let mut offset = 0;
    for line in content.split_inclusive('\n') {
        let line_body = line.strip_suffix('\n').unwrap_or(line);
        if line_body.trim() == marker {
            found = Some(offset);
        }
        offset += line.len();
    }
    found.ok_or_else(|| anyhow::anyhow!("marker `{marker}` not found"))
}

fn insert_before_marker(content: &str, marker: &str, insertion: &str) -> Result<String> {
    if marker_block(content, marker)
        .is_some_and(|block| block_contains_trimmed_lines(block, insertion))
    {
        return Ok(content.to_string());
    }
    let index = find_unique_marker_line(content, marker)?;
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

fn render_route_stub(name: &str, method: &str, path: Option<&str>) -> Result<String> {
    let module = name.to_snake_case();
    ensure_snake_identifier(&module, "route name")?;
    validate_route_method(method)?;
    let route_path = path
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("/api/{}", module.replace('_', "-")));
    if let Some(path) = path {
        validate_route_path(path)?;
    }
    let const_name = format!("{}_PATH", module.to_uppercase());
    Ok(format!(
        "pub const {const_name}: &str = \"{route_path}\";\npub const METHOD: &str = \"{}\";\n",
        method.to_ascii_uppercase()
    ))
}

const ROUTE_PATH_CHARS: &str = "/-_.{}:";
const HTTP_METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];

fn validate_route_method(method: &str) -> Result<()> {
    let upper = method.to_ascii_uppercase();
    if HTTP_METHODS.contains(&upper.as_str()) {
        Ok(())
    } else {
        anyhow::bail!(
            "HTTP method `{method}` is not supported; use one of {}",
            HTTP_METHODS.join(", ")
        )
    }
}

fn validate_route_path(path: &str) -> Result<()> {
    if !path.starts_with('/') {
        anyhow::bail!("route path `{path}` must start with `/`");
    }
    if path.contains('\\') || path.contains('"') || path.chars().any(char::is_whitespace) {
        anyhow::bail!("route path `{path}` contains characters that cannot be embedded in code");
    }
    let safe = path
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ROUTE_PATH_CHARS.contains(ch));
    if safe {
        Ok(())
    } else {
        anyhow::bail!("route path `{path}` contains unsupported characters")
    }
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
    let mut doc = text
        .parse::<DocumentMut>()
        .with_context(|| format!("{} is not valid TOML", path.display()))?;

    let dependency = doc
        .get_mut("dependencies")
        .and_then(Item::as_table_like_mut)
        .ok_or_else(|| anyhow::anyhow!("Cargo.toml has no [dependencies] table"))?
        .get_mut("ddd_cqrs_es")
        .ok_or_else(|| anyhow::anyhow!("Cargo.toml has no ddd_cqrs_es dependency"))?;
    let existing = dependency
        .as_table_like_mut()
        .and_then(|dependency| dependency.get_mut("features"))
        .and_then(Item::as_array_mut)
        .ok_or_else(|| anyhow::anyhow!("the ddd_cqrs_es dependency must use a features array"))?;

    for feature in features {
        if existing
            .iter()
            .any(|item| item.as_str() == Some(feature.as_str()))
        {
            continue;
        }
        existing.push(feature.clone());
    }

    Ok(doc.to_string())
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
