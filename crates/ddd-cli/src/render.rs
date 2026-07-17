use crate::manifest::{DomainRecord, ProjectManifest, MANIFEST_FILE};
use crate::model::{AppSelection, Preset};
use crate::operation::{write_operation, FileOperation};
use heck::{ToSnakeCase, ToUpperCamelCase};
use include_dir::{include_dir, Dir};

static TEMPLATE_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates");

fn framework_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Clone, Debug)]
pub struct InitRenderInput {
    pub package_name: String,
    pub domain_name: String,
    pub selection: AppSelection,
}

#[derive(Clone, Debug)]
pub struct NameParts {
    pub aggregate: String,
    pub module: String,
    pub id_type: String,
    pub command_type: String,
    pub event_type: String,
    pub create_command: String,
    pub created_event: String,
    pub created_event_name: String,
}

impl NameParts {
    pub fn new(domain_name: &str) -> Self {
        let aggregate = domain_name.to_upper_camel_case();
        let module = aggregate.to_snake_case();
        Self {
            id_type: format!("{aggregate}Id"),
            command_type: format!("{aggregate}Command"),
            event_type: format!("{aggregate}Event"),
            create_command: format!("Create{aggregate}"),
            created_event: format!("{aggregate}Created"),
            created_event_name: format!("{}_created", aggregate.to_snake_case()),
            aggregate,
            module,
        }
    }

    pub fn domain_record(&self) -> DomainRecord {
        DomainRecord {
            module: self.module.clone(),
            aggregate: self.aggregate.clone(),
            commands: vec![self.create_command.clone()],
            events: vec![self.created_event.clone()],
        }
    }
}

pub fn available_template_names() -> Vec<String> {
    TEMPLATE_DIR
        .dirs()
        .map(|dir| dir.path().display().to_string())
        .collect()
}

pub fn render_init(input: &InitRenderInput) -> Vec<FileOperation> {
    let names = NameParts::new(&input.domain_name);
    let manifest = ProjectManifest::new(
        input.package_name.clone(),
        input.selection,
        names.domain_record(),
    );

    let mut operations = vec![
        write_operation(MANIFEST_FILE, manifest.to_toml(), false, "project manifest"),
        write_operation(
            "README.md",
            render_readme(input, &names),
            false,
            "project README",
        ),
    ];

    if input.selection.preset != Preset::Fullstack {
        operations.extend([
            write_operation(
                "src/domain/mod.rs",
                render_domain_mod(&names),
                false,
                "domain module registry",
            ),
            write_operation(
                format!("src/domain/{}.rs", names.module),
                render_aggregate(&names),
                false,
                "aggregate, command, and event module",
            ),
            write_operation(
                format!("tests/{}_domain.rs", names.module),
                render_domain_test(input, &names),
                false,
                "aggregate fixture test",
            ),
        ]);
    }

    match input.selection.preset {
        Preset::Basic | Preset::Custom => operations.extend(render_basic(input, &names)),
        Preset::LeptosWasi => operations.extend(render_leptos_wasi(input, &names)),
        Preset::Fullstack => operations.extend(render_fullstack(input)),
        Preset::NativeApi => operations.extend(render_native_api(input, &names)),
        Preset::Worker => operations.extend(render_worker(input, &names)),
    }

    operations
}

pub fn sanitize_package_name(raw: &str) -> String {
    let mut output = String::new();
    let mut previous_dash = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            output.push('-');
            previous_dash = true;
        }
    }
    let output = output.trim_matches('-').to_string();
    if output.is_empty() {
        "ddd-app".to_string()
    } else {
        output
    }
}

pub fn parse_field_specs(fields: &[String]) -> anyhow::Result<Vec<(String, String)>> {
    fields
        .iter()
        .map(|field| {
            let Some((name, ty)) = field.split_once(':') else {
                anyhow::bail!("field `{field}` must use name:type syntax");
            };
            if name.trim().is_empty() || ty.trim().is_empty() {
                anyhow::bail!("field `{field}` must use non-empty name and type");
            }
            Ok((name.trim().to_snake_case(), ty.trim().to_string()))
        })
        .collect()
}

pub fn render_event_variant(name: &str, fields: &[(String, String)]) -> String {
    let variant = name.to_upper_camel_case();
    if fields.is_empty() {
        format!("    {variant} {{}},\n")
    } else {
        let fields = fields
            .iter()
            .map(|(name, ty)| format!("{name}: {ty}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("    {variant} {{ {fields} }},\n")
    }
}

pub fn render_command_variant(name: &str, fields: &[(String, String)]) -> String {
    render_event_variant(name, fields)
}

pub fn render_event_type_arm(event_type: &str, variant: &str) -> String {
    format!(
        "            Self::{} {{ .. }} => \"{}\",\n",
        variant.to_upper_camel_case(),
        event_type
    )
}

pub fn render_command_handle_arm(command_type: &str, command: &str) -> String {
    let command = command.to_upper_camel_case();
    format!(
        "            {command_type}::{command} {{ .. }} => Err(\"{command} command handler is not implemented\".to_string()),\n"
    )
}

fn render_basic(input: &InitRenderInput, _names: &NameParts) -> Vec<FileOperation> {
    vec![
        write_operation(
            "Cargo.toml",
            render_basic_cargo(input),
            false,
            "basic Cargo manifest",
        ),
        write_operation(
            "src/lib.rs",
            "pub mod domain;\n",
            false,
            "library entrypoint",
        ),
        write_operation(
            "src/main.rs",
            format!(
                "fn main() {{\n    println!(\"{} scaffold generated by ddd\");\n}}\n",
                input.package_name
            ),
            false,
            "binary entrypoint",
        ),
    ]
}

fn render_leptos_wasi(input: &InitRenderInput, names: &NameParts) -> Vec<FileOperation> {
    vec![
        write_operation(
            "Cargo.toml",
            render_leptos_cargo(input),
            false,
            "Leptos WASI Cargo manifest",
        ),
        write_operation(
            "Makefile",
            render_runtime_makefile(),
            false,
            "runtime Makefile",
        ),
        write_operation("spin.toml", render_spin_toml(input), false, "Spin manifest"),
        write_operation(
            "spin.redis.toml",
            render_spin_redis_toml(input),
            false,
            "Spin Redis manifest",
        ),
        write_operation(
            "src/lib.rs",
            render_leptos_lib(),
            false,
            "WASI library entrypoint",
        ),
        write_operation(
            "src/app.rs",
            render_app_boundary(names),
            false,
            "Leptos app boundary",
        ),
        write_operation(
            "src/application.rs",
            render_application_boundary(names),
            false,
            "shared application service boundary",
        ),
        write_operation(
            "src/store.rs",
            render_store_boundary(),
            false,
            "store boundary",
        ),
        write_operation(
            "src/rest.rs",
            render_rest_boundary(names),
            false,
            "REST boundary",
        ),
        write_operation(
            "src/server.rs",
            render_server_boundary(),
            false,
            "WASI server boundary",
        ),
        write_operation(
            "proto/service.proto",
            render_proto(names),
            false,
            "gRPC proto scaffold",
        ),
    ]
}

fn render_fullstack(input: &InitRenderInput) -> Vec<FileOperation> {
    let cargo_template = TEMPLATE_DIR
        .get_file("fullstack/Cargo.toml.template")
        .expect("fullstack Cargo.toml must be embedded")
        .contents_utf8()
        .expect("fullstack Cargo.toml must be UTF-8");
    let cargo = cargo_template
        .replacen(
            "name = \"fullstack-app\"",
            &format!("name = \"{}\"", input.package_name),
            1,
        )
        .replace(
            // Keep in sync with templates/fullstack/Cargo.toml.template pin; rewritten to CLI version.
            "ddd_cqrs_es = { version = \"=0.3.0-rc.5\"",
            &format!("ddd_cqrs_es = {{ version = \"={}\"", framework_version()),
        )
        .replace(
            "# Local wasi-auth for HTML mail templates until the next published rc.\nwasi-auth = { path = \"../../../wasi-auth/crates/wasi-auth\" }\n",
            "",
        );
    let mut operations = vec![write_operation(
        "Cargo.toml",
        cargo,
        false,
        "fullstack Cargo manifest",
    )];
    append_fullstack_template_operations(fullstack_template_dir(), input, &mut operations);
    operations
}

fn fullstack_template_dir() -> &'static Dir<'static> {
    TEMPLATE_DIR
        .get_dir("fullstack")
        .expect("fullstack template directory must be embedded")
}

fn append_fullstack_template_operations(
    dir: &Dir<'_>,
    input: &InitRenderInput,
    operations: &mut Vec<FileOperation>,
) {
    for file in dir.files() {
        let relative_path = file
            .path()
            .strip_prefix("fullstack")
            .expect("fullstack template files must be under fullstack");
        let relative_path_string = relative_path.display().to_string();
        // Cargo.toml is written from Cargo.toml.template (renamed so cargo package
        // does not treat templates/fullstack as a nested package and drop the tree).
        if relative_path_string == "README.md"
            || relative_path_string == "Cargo.toml"
            || relative_path_string == "Cargo.toml.template"
        {
            continue;
        }
        let content = render_fullstack_template_content(
            &relative_path_string,
            file.contents_utf8()
                .expect("fullstack template files must be UTF-8"),
            input,
        );
        operations.push(write_operation(
            relative_path,
            content,
            false,
            "fullstack template file",
        ));
    }

    for child in dir.dirs() {
        append_fullstack_template_operations(child, input, operations);
    }
}

fn render_fullstack_template_content(
    relative_path: &str,
    raw: &str,
    input: &InitRenderInput,
) -> String {
    let crate_name = input.package_name.replace('-', "_");
    let mut content = raw.replace("/pkg/fullstack_app.css", &format!("/pkg/{crate_name}.css"));

    if matches!(
        relative_path,
        "spin.toml"
            | "spin.production.toml.example"
            | "package.json"
            | "package-lock.json"
            | "scripts/verify_wasi_artifact.sh"
    ) {
        content = content.replace("fullstack_app.wasm", &format!("{crate_name}.wasm"));
        content = content.replace(
            "LEPTOS_OUTPUT_NAME=fullstack_app",
            &format!("LEPTOS_OUTPUT_NAME={crate_name}"),
        );
        content = content.replace(
            r#""name": "fullstack-app""#,
            &format!(r#""name": "{}""#, input.package_name),
        );
        content = content.replace(
            r#"name = "fullstack-app""#,
            &format!(r#"name = "{}""#, input.package_name),
        );
    }

    content
}

fn render_native_api(input: &InitRenderInput, names: &NameParts) -> Vec<FileOperation> {
    vec![
        write_operation(
            "Cargo.toml",
            render_native_api_cargo(input),
            false,
            "native API Cargo manifest",
        ),
        write_operation(
            "src/lib.rs",
            "pub mod domain;\n",
            false,
            "library entrypoint",
        ),
        write_operation(
            "src/main.rs",
            render_native_api_main(names),
            false,
            "Axum-style API entrypoint",
        ),
    ]
}

fn render_worker(input: &InitRenderInput, names: &NameParts) -> Vec<FileOperation> {
    vec![
        write_operation(
            "Cargo.toml",
            render_worker_cargo(input),
            false,
            "worker Cargo manifest",
        ),
        write_operation(
            "src/lib.rs",
            "pub mod domain;\n",
            false,
            "library entrypoint",
        ),
        write_operation(
            "src/main.rs",
            render_worker_main(names),
            false,
            "projection worker entrypoint",
        ),
    ]
}

fn render_readme(input: &InitRenderInput, names: &NameParts) -> String {
    if input.selection.preset == Preset::Fullstack {
        // Ship the dual-synced product README from templates/fullstack/README.md
        // (kept in lockstep with examples/fullstack-app via sync_fullstack_template.sh).
        let raw = TEMPLATE_DIR
            .get_file("fullstack/README.md")
            .expect("fullstack README.md must be embedded")
            .contents_utf8()
            .expect("fullstack README.md must be UTF-8");
        return raw.replacen("# fullstack-app", &format!("# {}", input.package_name), 1);
    }
    format!(
        "# {}\n\nGenerated with `ddd init --preset {}`.\n\n- Aggregate: `{}`\n- Runtime: `{}`\n- DB: `{}`\n- Realtime: `{}`\n- Transport: `{}`\n\nUse `ddd add ...` to extend this generated project.\n",
        input.package_name,
        input.selection.preset,
        names.aggregate,
        input.selection.runtime,
        input.selection.db,
        input.selection.realtime,
        input.selection.transport
    )
}

fn render_basic_cargo(input: &InitRenderInput) -> String {
    format!(
        r#"[package]
name = "{package}"
version = "0.1.0"
edition = "2021"

[dependencies]
ddd_cqrs_es = {{ version = "{framework_version}", features = ["serde", "json"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
"#,
        package = input.package_name,
        framework_version = framework_version()
    )
}

fn render_leptos_cargo(input: &InitRenderInput) -> String {
    format!(
        r#"[package]
name = "{package}"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
ddd_cqrs_es = {{ version = "{framework_version}", default-features = false, features = ["json", "serde", "async", "tracing", "{db_feature}"] }}
anyhow = "1"
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
thiserror = "2"
tracing = "0.1"

[features]
default = []
hydrate = []
ssr = []
sqlite = ["ddd_cqrs_es/sqlite"]
postgres = ["ddd_cqrs_es/postgres"]
mysql = ["ddd_cqrs_es/mysql"]
redis = ["ddd_cqrs_es/redis"]
spin-grpc = []
"#,
        package = input.package_name,
        framework_version = framework_version(),
        db_feature = input.selection.db.feature(input.selection.runtime)
    )
}

fn render_native_api_cargo(input: &InitRenderInput) -> String {
    format!(
        r#"[package]
name = "{package}"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.8"
ddd_cqrs_es = {{ version = "{framework_version}", features = ["serde", "json", "{db_feature}"] }}
serde = {{ version = "1", features = ["derive"] }}
tokio = {{ version = "1", features = ["macros", "rt-multi-thread"] }}
"#,
        package = input.package_name,
        framework_version = framework_version(),
        db_feature = input.selection.db.feature(input.selection.runtime)
    )
}

fn render_worker_cargo(input: &InitRenderInput) -> String {
    format!(
        r#"[package]
name = "{package}"
version = "0.1.0"
edition = "2021"

[dependencies]
ddd_cqrs_es = {{ version = "{framework_version}", features = ["serde", "json", "async", "{db_feature}"] }}
serde = {{ version = "1", features = ["derive"] }}
tokio = {{ version = "1", features = ["macros", "rt-multi-thread"] }}
"#,
        package = input.package_name,
        framework_version = framework_version(),
        db_feature = input.selection.db.feature(input.selection.runtime)
    )
}

pub fn render_domain_mod(names: &NameParts) -> String {
    format!(
        "pub mod {module};\n// ddd:domain-modules\n// ddd:domain-modules:end\n\npub use {module}::{{{aggregate}, {command_type}, {event_type}, {id_type}}};\n// ddd:domain-exports\n// ddd:domain-exports:end\n",
        module = names.module,
        aggregate = names.aggregate,
        command_type = names.command_type,
        event_type = names.event_type,
        id_type = names.id_type
    )
}

pub fn render_aggregate(names: &NameParts) -> String {
    format!(
        r#"use ddd_cqrs_es::{{Aggregate, DomainEvent}};
use serde::{{Deserialize, Serialize}};
use std::fmt;

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct {id_type}(pub String);

impl fmt::Display for {id_type} {{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {{
        f.write_str(&self.0)
    }}
}}

impl From<String> for {id_type} {{
    fn from(value: String) -> Self {{
        Self(value)
    }}
}}

impl From<&str> for {id_type} {{
    fn from(value: &str) -> Self {{
        Self(value.to_string())
    }}
}}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum {command_type} {{
    {create_command} {{ name: String }},
    // ddd:commands
    // ddd:commands:end
}}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum {event_type} {{
    {created_event} {{ name: String }},
    // ddd:events
    // ddd:events:end
}}

impl DomainEvent for {event_type} {{
    fn event_type(&self) -> &'static str {{
        match self {{
            Self::{created_event} {{ .. }} => "{created_event_name}",
            // ddd:event-types
            // ddd:event-types:end
        }}
    }}
}}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct {aggregate} {{
    pub id: {id_type},
    pub name: Option<String>,
    pub revision: u64,
}}

impl Aggregate for {aggregate} {{
    type Id = {id_type};
    type Command = {command_type};
    type Event = {event_type};
    type Error = String;

    fn aggregate_type() -> &'static str {{
        "{module}"
    }}

    fn revision(&self) -> u64 {{
        self.revision
    }}

    fn new() -> Self {{
        Self {{
            id: {id_type}(String::new()),
            name: None,
            revision: 0,
        }}
    }}

    fn apply(&mut self, event: &Self::Event) {{
        match event {{
            {event_type}::{created_event} {{ name }} => self.name = Some(name.clone()),
            // ddd:apply-events
            // ddd:apply-events:end
        }}
        self.revision += 1;
    }}

    fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {{
        match command {{
            {command_type}::{create_command} {{ name }} => {{
                if name.trim().is_empty() {{
                    return Err("name must not be empty".to_string());
                }}
                Ok(vec![{event_type}::{created_event} {{ name }}])
            }}
            // ddd:handle-commands
            // ddd:handle-commands:end
        }}
    }}
}}
"#,
        aggregate = names.aggregate,
        id_type = names.id_type,
        command_type = names.command_type,
        event_type = names.event_type,
        create_command = names.create_command,
        created_event = names.created_event,
        created_event_name = names.created_event_name,
        module = names.module
    )
}

pub fn render_domain_test(input: &InitRenderInput, names: &NameParts) -> String {
    format!(
        r#"use ddd_cqrs_es::Aggregate;
use {module_path}::domain::{{{aggregate}, {command_type}, {event_type}}};

#[test]
fn create_command_emits_created_event() {{
    let aggregate = {aggregate}::new();
    let events = aggregate
        .handle({command_type}::{create_command} {{
            name: "example".to_string(),
        }})
        .unwrap();

    assert_eq!(
        events,
        vec![{event_type}::{created_event} {{
            name: "example".to_string()
        }}]
    );
}}
"#,
        module_path = input.package_name.replace('-', "_"),
        aggregate = names.aggregate,
        command_type = names.command_type,
        event_type = names.event_type,
        create_command = names.create_command,
        created_event = names.created_event
    )
}

fn render_native_api_main(names: &NameParts) -> String {
    format!(
        r#"use axum::{{routing::get, Router}};

#[tokio::main]
async fn main() {{
    let app = Router::new().route("/health", get(|| async {{ "ok" }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("bind API listener");
    println!("serving {aggregate} API at http://127.0.0.1:3000");
    axum::serve(listener, app).await.expect("serve API");
}}
"#,
        aggregate = names.aggregate
    )
}

fn render_worker_main(names: &NameParts) -> String {
    format!(
        r#"#[tokio::main]
async fn main() {{
    println!("starting {aggregate} projection worker");
}}
"#,
        aggregate = names.aggregate
    )
}

fn render_runtime_makefile() -> String {
    r#".DEFAULT_GOAL := help
DB_BACKENDS := sqlite postgres neon supabase turso mysql redis
REALTIME_BACKENDS := off polling redis
TRANSPORTS := http grpc both

db ?= sqlite
realtime ?= off
transport ?= http

help:
	@echo "Usage: make <spin|fresh> db=<backend> realtime=<mode> transport=<mode>"
	@echo "db=$(DB_BACKENDS)"
	@echo "realtime=$(REALTIME_BACKENDS)"
	@echo "transport=$(TRANSPORTS)"

validate:
	@case "$(db)" in sqlite|postgres|neon|supabase|turso|mysql|redis) ;; *) echo "unsupported db=$(db)"; exit 2 ;; esac
	@case "$(realtime)" in off|polling|redis) ;; *) echo "unsupported realtime=$(realtime)"; exit 2 ;; esac
	@case "$(transport)" in http|grpc|both) ;; *) echo "unsupported transport=$(transport)"; exit 2 ;; esac

spin: validate
	@echo "Spin serve scaffold: db=$(db) realtime=$(realtime) transport=$(transport)"

fresh: validate
	@echo "Reset scaffold only: db=$(db)"
"#
    .to_string()
}

fn render_spin_toml(input: &InitRenderInput) -> String {
    format!(
        r#"spin_manifest_version = 2

[application]
name = "{package}"
version = "0.1.0"

[[trigger.http]]
route = "/..."
component = "{package}"

[component.{package}]
source = "target/wasm32-wasip2/release/{crate_name}.wasm"
allowed_outbound_hosts = []
"#,
        package = input.package_name,
        crate_name = input.package_name.replace('-', "_")
    )
}

fn render_spin_redis_toml(input: &InitRenderInput) -> String {
    format!(
        r#"spin_manifest_version = 2

[application]
name = "{package}-redis"
version = "0.1.0"

[variables]
redis_url = {{ default = "redis://127.0.0.1:6379" }}
redis_channel = {{ default = "counter-events" }}

[[trigger.http]]
route = "/..."
component = "{package}"
"#,
        package = input.package_name
    )
}

fn render_leptos_lib() -> String {
    "pub mod app;\npub mod application;\npub mod domain;\npub mod rest;\npub mod server;\npub mod store;\n".to_string()
}

fn render_app_boundary(names: &NameParts) -> String {
    format!(
        r#"use serde::{{Deserialize, Serialize}};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct {aggregate}ViewDto {{
    pub id: String,
    pub name: Option<String>,
    pub last_sequence: u64,
    pub realtime_enabled: bool,
}}

// ddd:server-functions
// ddd:server-functions:end
"#,
        aggregate = names.aggregate
    )
}

fn render_application_boundary(names: &NameParts) -> String {
    format!(
        r#"use crate::app::{aggregate}ViewDto;
use crate::domain::{{{aggregate}, {command_type}, {id_type}}};

pub async fn get_{module}_view(id: {id_type}) -> anyhow::Result<{aggregate}ViewDto> {{
    let _ = id;
    anyhow::bail!("wire a read model for {aggregate}")
}}

pub async fn execute_{module}_command(
    id: {id_type},
    command: {command_type},
) -> anyhow::Result<{aggregate}ViewDto> {{
    let _ = (id, command);
    anyhow::bail!("wire AsyncRepository<{aggregate}, _> and shared command execution")
}}
"#,
        aggregate = names.aggregate,
        command_type = names.command_type,
        id_type = names.id_type,
        module = names.module
    )
}

fn render_store_boundary() -> String {
    "pub fn backend() -> String {\n    std::env::var(\"DATABASE_BACKEND\").unwrap_or_else(|_| \"sqlite\".to_string())\n}\n".to_string()
}

fn render_rest_boundary(names: &NameParts) -> String {
    format!(
        r#"pub const VIEW_PATH: &str = "/api/{module}/view";
// ddd:rest-routes
// ddd:rest-routes:end
"#,
        module = names.module
    )
}

fn render_server_boundary() -> String {
    r#"pub fn transport_mode() -> String {
    std::env::var("TRANSPORT_MODE").unwrap_or_else(|_| "http".to_string())
}

// ddd:server-routes
// ddd:server-routes:end
"#
    .to_string()
}

fn render_proto(names: &NameParts) -> String {
    format!(
        r#"syntax = "proto3";

package {module}.v1;

service {aggregate}Service {{
  rpc Get{aggregate}View(Get{aggregate}ViewRequest) returns ({aggregate}View);
  // ddd:grpc-methods
  // ddd:grpc-methods:end
}}

message Get{aggregate}ViewRequest {{
  string id = 1;
}}

message {aggregate}View {{
  string id = 1;
  string name = 2;
  uint64 last_sequence = 3;
  bool realtime_enabled = 4;
}}
"#,
        aggregate = names.aggregate,
        module = names.module
    )
}

/// Fullstack product-domain application service (InMemory demo store).
pub fn render_fullstack_domain_app_module(names: &NameParts) -> String {
    format!(
        r#"//! Application service for the `{aggregate}` product domain aggregate.
//!
//! Uses a **process-local** `InMemoryEventStore` so REST demos compile and
//! round-trip inside a single Spin instance. Replace with a durable Spin/SQL
//! store before production. Routes under `/api/domain/` are not Cedar-gated;
//! add authn/authz when exposing publicly.

use crate::domain::{{{aggregate}, {command_type}, {id_type}}};
use ddd_cqrs_es::{{AsyncRepository, InMemoryEventStore, Metadata}};
use serde::{{Deserialize, Serialize}};
use std::sync::OnceLock;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct {aggregate}ViewDto {{
    pub id: String,
    pub name: Option<String>,
    pub revision: u64,
}}

fn store() -> InMemoryEventStore<{aggregate}> {{
    static STORE: OnceLock<InMemoryEventStore<{aggregate}>> = OnceLock::new();
    STORE.get_or_init(InMemoryEventStore::new).clone()
}}

fn repo() -> AsyncRepository<{aggregate}, InMemoryEventStore<{aggregate}>> {{
    AsyncRepository::new(store())
}}

pub async fn get_{module}_view(id: {id_type}) -> Result<{aggregate}ViewDto, String> {{
    let stream_id = id.to_string();
    let loaded = repo()
        .load(&id)
        .await
        .map_err(|error| error.to_string())?;
    Ok({aggregate}ViewDto {{
        id: stream_id,
        name: loaded.state.name.clone(),
        revision: loaded.state.revision,
    }})
}}

pub async fn execute_{module}_command(
    id: {id_type},
    command: {command_type},
) -> Result<{aggregate}ViewDto, String> {{
    let stream_id = id.to_string();
    let (loaded, _events) = repo()
        .execute_returning_state(&id, command, Metadata::new())
        .await
        .map_err(|error| error.to_string())?;
    Ok({aggregate}ViewDto {{
        id: stream_id,
        name: loaded.state.name.clone(),
        revision: loaded.state.revision,
    }})
}}
"#,
        aggregate = names.aggregate,
        command_type = names.command_type,
        id_type = names.id_type,
        module = names.module
    )
}

pub fn render_fullstack_domain_app_mod(names: &NameParts) -> String {
    format!(
        r#"//! Product-domain application services (business aggregates beside wasi-auth).
//!
//! Generated by `ddd add aggregate`. Dual-sync excludes this directory.

pub mod {module};
// ddd:domain-app-modules
// ddd:domain-app-modules:end

pub use {module}::*;
// ddd:domain-app-exports
// ddd:domain-app-exports:end
"#,
        module = names.module
    )
}

pub fn render_fullstack_domain_rest_bootstrap(names: &NameParts) -> String {
    format!(
        r#"//! REST surface for product-domain aggregates (`/api/domain/...`).
//!
//! Generated by `ddd add aggregate`. Demo endpoints are not Cedar-gated.

use bytes::Bytes;
use http::{{Method, StatusCode}};
use http_body_util::{{BodyExt, StreamBody, combinators::UnsyncBoxBody}};
use serde::Serialize;

use crate::domain::{{{command_type}, {id_type}}};
use crate::domain_app;
use crate::error::{{AuthStackError, AuthStackResult}};

type RestBody = UnsyncBoxBody<Bytes, std::io::Error>;
type RestResponse = http::Response<RestBody>;
type RestRequest = http::Request<wasip3::http_compat::IncomingRequestBody>;

pub fn is_domain_rest_path(path: &str) -> bool {{
    path.starts_with("/api/domain/")
}}

pub async fn dispatch(req: RestRequest) -> AuthStackResult<RestResponse> {{
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    match (method, path.as_str()) {{
        (Method::GET, path) if path.starts_with("/api/domain/{module}/") => {{
            let id = path
                .strip_prefix("/api/domain/{module}/")
                .unwrap_or_default()
                .trim_matches('/')
                .to_string();
            if id.is_empty() || id.contains('/') {{
                return Err(AuthStackError::not_found("unknown domain route"));
            }}
            let view = domain_app::get_{module}_view({id_type}::from(id))
                .await
                .map_err(AuthStackError::store)?;
            json_ok(&view)
        }}
        (Method::POST, path)
            if path.starts_with("/api/domain/{module}/") && path.ends_with("/commands") =>
        {{
            let trimmed = path
                .strip_prefix("/api/domain/{module}/")
                .and_then(|rest| rest.strip_suffix("/commands"))
                .unwrap_or_default()
                .trim_matches('/');
            if trimmed.is_empty() || trimmed.contains('/') {{
                return Err(AuthStackError::not_found("unknown domain route"));
            }}
            let command = parse_json::<{command_type}>(req).await?;
            let view = domain_app::execute_{module}_command(
                {id_type}::from(trimmed.to_string()),
                command,
            )
            .await
            .map_err(AuthStackError::store)?;
            json_ok(&view)
        }}
        // ddd:domain-rest-arms
        // ddd:domain-rest-arms:end
        _ => Err(AuthStackError::not_found("unknown domain API route")),
    }}
}}

async fn parse_json<T: serde::de::DeserializeOwned>(req: RestRequest) -> AuthStackResult<T> {{
    let mut incoming = req.into_body();
    let mut body = Vec::new();
    while let Some(frame) = incoming.frame().await {{
        let frame = frame.map_err(|error| {{
            AuthStackError::transport(format!("failed to read request body: {{error:?}}"))
        }})?;
        let Ok(data) = frame.into_data() else {{
            continue;
        }};
        body.extend_from_slice(&data);
    }}
    if body.is_empty() {{
        return Err(AuthStackError::validation("JSON body is required"));
    }}
    serde_json::from_slice(&body)
        .map_err(|error| AuthStackError::validation(format!("invalid JSON body: {{error}}")))
}}

fn json_ok<T: Serialize>(value: &T) -> AuthStackResult<RestResponse> {{
    let bytes = serde_json::to_vec(value)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    let stream = futures::stream::once(async move {{
        Ok::<_, std::io::Error>(http_body::Frame::data(Bytes::from(bytes)))
    }});
    let body = StreamBody::new(stream).boxed_unsync();
    http::Response::builder()
        .status(StatusCode::OK)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(body)
        .map_err(|error| AuthStackError::transport(error.to_string()))
}}
"#,
        module = names.module,
        command_type = names.command_type,
        id_type = names.id_type
    )
}

pub fn render_fullstack_domain_rest_arm(names: &NameParts) -> String {
    format!(
        r#"        (Method::GET, path) if path.starts_with("/api/domain/{module}/") => {{
            let id = path
                .strip_prefix("/api/domain/{module}/")
                .unwrap_or_default()
                .trim_matches('/')
                .to_string();
            if id.is_empty() || id.contains('/') {{
                return Err(AuthStackError::not_found("unknown domain route"));
            }}
            let view = domain_app::get_{module}_view({id_type}::from(id))
                .await
                .map_err(AuthStackError::store)?;
            json_ok(view)
        }}
        (Method::POST, path) if path.starts_with("/api/domain/{module}/") && path.ends_with("/commands") => {{
            let trimmed = path
                .strip_prefix("/api/domain/{module}/")
                .and_then(|rest| rest.strip_suffix("/commands"))
                .unwrap_or_default()
                .trim_matches('/');
            if trimmed.is_empty() || trimmed.contains('/') {{
                return Err(AuthStackError::not_found("unknown domain route"));
            }}
            let command = parse_json::<{command_type}>(req).await?;
            let view = domain_app::execute_{module}_command({id_type}::from(trimmed.to_string()), command)
                .await
                .map_err(AuthStackError::store)?;
            json_ok(view)
        }}
"#,
        module = names.module,
        command_type = names.command_type,
        id_type = names.id_type
    )
}
