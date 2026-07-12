use crate::model::{
    parse_model_value, AppSelection, DbBackend, OAuthProviderKind, Preset, Realtime, Runtime,
    Transport, Ui,
};
use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use toml_edit::{value, Array, ArrayOfTables, DocumentMut, Item, Table};

pub const MANIFEST_FILE: &str = "ddd.toml";

#[derive(Clone, Debug, Serialize)]
pub struct ProjectManifest {
    pub name: String,
    pub preset: Preset,
    pub runtime: Runtime,
    pub db: DbBackend,
    pub realtime: Realtime,
    pub transport: Transport,
    pub ui: Ui,
    pub capabilities: Vec<String>,
    pub auth: Option<AuthConfig>,
    pub authorization: Option<AuthorizationConfig>,
    pub domains: Vec<DomainRecord>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthConfig {
    pub issuer: String,
    pub audience: String,
    pub access_token_ttl_seconds: u64,
    pub refresh_token_ttl_seconds: u64,
    pub cookie_mode: String,
    pub providers: Vec<AuthProviderRecord>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthProviderRecord {
    pub provider_id: String,
    pub issuer: String,
    pub scopes: Vec<String>,
    pub enabled_env: String,
    pub client_id_env: String,
    pub client_secret_env: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthorizationConfig {
    pub provider: String,
    pub policy_revision: String,
    pub default_decision: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DomainRecord {
    pub module: String,
    pub aggregate: String,
    pub commands: Vec<String>,
    pub events: Vec<String>,
}

impl ProjectManifest {
    pub fn new(name: impl Into<String>, selection: AppSelection, domain: DomainRecord) -> Self {
        let name = name.into();
        let mut capabilities = Vec::new();
        if selection.ui == Ui::Leptos {
            capabilities.push("leptos".to_string());
            capabilities.push("server-fn".to_string());
            capabilities.push("rest".to_string());
        }
        if selection.transport != Transport::Http {
            capabilities.push("grpc".to_string());
        }
        if selection.realtime != Realtime::Off {
            capabilities.push(format!("realtime:{}", selection.realtime));
        }
        if selection.db == DbBackend::Redis {
            capabilities.push("redis-store".to_string());
        }
        let (auth, authorization) = if selection.preset == Preset::Fullstack {
            capabilities.push("auth".to_string());
            capabilities.push("authorization".to_string());
            (
                Some(AuthConfig::default_for_project(&name)),
                Some(AuthorizationConfig::embedded_cedar()),
            )
        } else {
            (None, None)
        };
        capabilities.sort();
        capabilities.dedup();
        let domains = if selection.preset == Preset::Fullstack {
            Vec::new()
        } else {
            vec![domain]
        };

        Self {
            name,
            preset: selection.preset,
            runtime: selection.runtime,
            db: selection.db,
            realtime: selection.realtime,
            transport: selection.transport,
            ui: selection.ui,
            capabilities,
            auth,
            authorization,
            domains,
        }
    }

    pub fn selection(&self) -> AppSelection {
        AppSelection {
            preset: self.preset,
            runtime: self.runtime,
            db: self.db,
            realtime: self.realtime,
            transport: self.transport,
            ui: self.ui,
        }
    }

    pub fn manifest_path(cwd: &Path) -> PathBuf {
        cwd.join(MANIFEST_FILE)
    }

    pub fn read_from(cwd: &Path) -> Result<Self> {
        let path = Self::manifest_path(cwd);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        Self::from_toml(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn to_toml(&self) -> String {
        let mut doc = DocumentMut::new();
        doc["project"]["name"] = value(self.name.as_str());
        doc["project"]["preset"] = value(self.preset.as_str());
        doc["project"]["runtime"] = value(self.runtime.as_str());
        doc["project"]["db"] = value(self.db.as_str());
        doc["project"]["realtime"] = value(self.realtime.as_str());
        doc["project"]["transport"] = value(self.transport.as_str());
        doc["project"]["ui"] = value(self.ui.as_str());

        let mut enabled = Array::default();
        for capability in &self.capabilities {
            enabled.push(capability.as_str());
        }
        doc["capabilities"]["enabled"] = value(enabled);

        if let Some(auth) = &self.auth {
            doc["auth"] = Item::Table(Table::new());
            doc["auth"]["issuer"] = value(auth.issuer.as_str());
            doc["auth"]["audience"] = value(auth.audience.as_str());
            doc["auth"]["access_token_ttl_seconds"] = value(auth.access_token_ttl_seconds as i64);
            doc["auth"]["refresh_token_ttl_seconds"] = value(auth.refresh_token_ttl_seconds as i64);
            doc["auth"]["cookie_mode"] = value(auth.cookie_mode.as_str());

            if !auth.providers.is_empty() {
                let mut providers = ArrayOfTables::new();
                for provider in &auth.providers {
                    let mut table = Table::new();
                    table["provider_id"] = value(provider.provider_id.as_str());
                    table["issuer"] = value(provider.issuer.as_str());

                    let mut scopes = Array::default();
                    for scope in &provider.scopes {
                        scopes.push(scope.as_str());
                    }
                    table["scopes"] = value(scopes);
                    table["enabled_env"] = value(provider.enabled_env.as_str());
                    table["client_id_env"] = value(provider.client_id_env.as_str());
                    table["client_secret_env"] = value(provider.client_secret_env.as_str());
                    providers.push(table);
                }
                doc["auth"]["providers"] = Item::ArrayOfTables(providers);
            }
        }

        if let Some(authorization) = &self.authorization {
            doc["authorization"] = Item::Table(Table::new());
            doc["authorization"]["provider"] = value(authorization.provider.as_str());
            doc["authorization"]["policy_revision"] = value(authorization.policy_revision.as_str());
            doc["authorization"]["default_decision"] =
                value(authorization.default_decision.as_str());
        }

        doc["domains"] = Item::Table(Table::new());
        for domain in &self.domains {
            doc["domains"][domain.module.as_str()]["aggregate"] = value(domain.aggregate.as_str());
            doc["domains"][domain.module.as_str()]["module"] = value(domain.module.as_str());

            let mut commands = Array::default();
            for command in &domain.commands {
                commands.push(command.as_str());
            }
            doc["domains"][domain.module.as_str()]["commands"] = value(commands);

            let mut events = Array::default();
            for event in &domain.events {
                events.push(event.as_str());
            }
            doc["domains"][domain.module.as_str()]["events"] = value(events);
        }

        doc.to_string()
    }

    pub fn from_toml(text: &str) -> Result<Self> {
        let doc = text.parse::<DocumentMut>()?;
        let project = &doc["project"];
        let name = required_str(project, "name")?.to_string();
        let preset = parse_model_value(required_str(project, "preset")?, "preset")?;
        let runtime = parse_model_value(required_str(project, "runtime")?, "runtime")?;
        let db = parse_model_value(required_str(project, "db")?, "db")?;
        let realtime = parse_model_value(required_str(project, "realtime")?, "realtime")?;
        let transport = parse_model_value(required_str(project, "transport")?, "transport")?;
        let ui = parse_model_value(required_str(project, "ui")?, "ui")?;

        let capabilities = doc["capabilities"]["enabled"]
            .as_array()
            .map(|array| {
                array
                    .iter()
                    .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let auth = doc
            .get("auth")
            .and_then(Item::as_table)
            .map(AuthConfig::from_table);
        let authorization = doc
            .get("authorization")
            .and_then(Item::as_table)
            .map(AuthorizationConfig::from_table)
            .or_else(|| {
                doc.get("authz")
                    .and_then(Item::as_table)
                    .map(AuthorizationConfig::from_legacy_table)
            });

        let mut domains = Vec::new();
        if let Some(domain_table) = doc["domains"].as_table() {
            for (module, item) in domain_table {
                let aggregate = item["aggregate"].as_str().unwrap_or(module).to_string();
                let commands = string_array(&item["commands"]);
                let events = string_array(&item["events"]);
                domains.push(DomainRecord {
                    module: module.to_string(),
                    aggregate,
                    commands,
                    events,
                });
            }
        }

        Ok(Self {
            name,
            preset,
            runtime,
            db,
            realtime,
            transport,
            ui,
            capabilities,
            auth,
            authorization,
            domains,
        })
    }

    pub fn enable_auth(&mut self) {
        self.add_capability("auth");
        if self.auth.is_none() {
            self.auth = Some(AuthConfig::default_for_project(&self.name));
        }
    }

    pub fn enable_authorization(&mut self) {
        self.add_capability("authorization");
        self.capabilities.retain(|capability| capability != "authz");
        if self.authorization.is_none() {
            self.authorization = Some(AuthorizationConfig::embedded_cedar());
        }
    }

    pub fn enable_passkeys(&mut self) {
        self.enable_auth();
        self.add_capability("passkeys");
    }

    pub fn enable_oauth_provider(&mut self, provider: OAuthProviderKind) {
        self.enable_auth();
        self.add_capability(format!("oauth:{}", provider.as_str()));
        let auth = self
            .auth
            .get_or_insert_with(|| AuthConfig::default_for_project(&self.name));
        auth.add_provider(provider);
    }

    pub fn set_db(&mut self, db: DbBackend) {
        self.db = db;
        if db == DbBackend::Redis {
            self.add_capability("redis-store");
        }
    }

    pub fn set_realtime(&mut self, realtime: Realtime) {
        self.realtime = realtime;
        if realtime != Realtime::Off {
            self.add_capability(format!("realtime:{realtime}"));
        }
    }

    pub fn set_transport(&mut self, transport: Transport) {
        self.transport = transport;
        if transport != Transport::Http {
            self.add_capability("grpc");
        }
    }

    pub fn add_capability(&mut self, capability: impl Into<String>) {
        let capability = capability.into();
        if !self.capabilities.iter().any(|item| item == &capability) {
            self.capabilities.push(capability);
            self.capabilities.sort();
        }
    }

    pub fn add_domain(&mut self, domain: DomainRecord) {
        if !self
            .domains
            .iter()
            .any(|existing| existing.module == domain.module)
        {
            self.domains.push(domain);
            self.domains.sort_by(|a, b| a.module.cmp(&b.module));
        }
    }

    pub fn add_event(&mut self, module: &str, event: &str) {
        if let Some(domain) = self
            .domains
            .iter_mut()
            .find(|domain| domain.module == module)
        {
            push_unique(&mut domain.events, event);
        }
    }

    pub fn add_command(&mut self, module: &str, command: &str) {
        if let Some(domain) = self
            .domains
            .iter_mut()
            .find(|domain| domain.module == module)
        {
            push_unique(&mut domain.commands, command);
        }
    }
}

impl AuthConfig {
    fn default_for_project(name: &str) -> Self {
        Self {
            issuer: "http://127.0.0.1:3008".to_string(),
            audience: name.to_string(),
            access_token_ttl_seconds: 900,
            refresh_token_ttl_seconds: 2_592_000,
            cookie_mode: "http-only".to_string(),
            providers: Vec::new(),
        }
    }

    fn from_table(table: &Table) -> Self {
        let mut config = Self {
            issuer: table
                .get("issuer")
                .and_then(Item::as_str)
                .unwrap_or("http://127.0.0.1:3008")
                .to_string(),
            audience: table
                .get("audience")
                .and_then(Item::as_str)
                .unwrap_or("fullstack-app")
                .to_string(),
            access_token_ttl_seconds: table
                .get("access_token_ttl_seconds")
                .and_then(Item::as_integer)
                .and_then(|value| u64::try_from(value).ok())
                .unwrap_or(900),
            refresh_token_ttl_seconds: table
                .get("refresh_token_ttl_seconds")
                .and_then(Item::as_integer)
                .and_then(|value| u64::try_from(value).ok())
                .unwrap_or(2_592_000),
            cookie_mode: table
                .get("cookie_mode")
                .and_then(Item::as_str)
                .unwrap_or("http-only")
                .to_string(),
            providers: Vec::new(),
        };
        if let Some(providers) = table.get("providers").and_then(Item::as_array_of_tables) {
            config.providers = providers
                .iter()
                .map(AuthProviderRecord::from_table)
                .collect();
        }
        config
    }

    fn add_provider(&mut self, provider: OAuthProviderKind) {
        let provider_id = provider.as_str();
        if self
            .providers
            .iter()
            .any(|existing| existing.provider_id == provider_id)
        {
            return;
        }
        self.providers.push(AuthProviderRecord::from_kind(provider));
        self.providers
            .sort_by(|left, right| left.provider_id.cmp(&right.provider_id));
    }
}

impl AuthProviderRecord {
    fn from_kind(provider: OAuthProviderKind) -> Self {
        Self {
            provider_id: provider.as_str().to_string(),
            issuer: provider.issuer().to_string(),
            scopes: provider
                .scopes()
                .iter()
                .map(|scope| (*scope).to_string())
                .collect(),
            enabled_env: provider.enabled_env().to_string(),
            client_id_env: provider.client_id_env().to_string(),
            client_secret_env: provider.client_secret_env().to_string(),
        }
    }

    fn from_table(table: &Table) -> Self {
        Self {
            provider_id: table
                .get("provider_id")
                .and_then(Item::as_str)
                .unwrap_or("")
                .to_string(),
            issuer: table
                .get("issuer")
                .and_then(Item::as_str)
                .unwrap_or("")
                .to_string(),
            scopes: table.get("scopes").map(string_array).unwrap_or_default(),
            enabled_env: table
                .get("enabled_env")
                .and_then(Item::as_str)
                .unwrap_or("")
                .to_string(),
            client_id_env: table
                .get("client_id_env")
                .and_then(Item::as_str)
                .unwrap_or("")
                .to_string(),
            client_secret_env: table
                .get("client_secret_env")
                .and_then(Item::as_str)
                .unwrap_or("")
                .to_string(),
        }
    }
}

impl AuthorizationConfig {
    fn embedded_cedar() -> Self {
        Self {
            provider: "embedded-cedar".to_string(),
            policy_revision: "embedded-v1".to_string(),
            default_decision: "deny".to_string(),
        }
    }

    fn from_table(table: &Table) -> Self {
        Self {
            provider: table
                .get("provider")
                .and_then(Item::as_str)
                .unwrap_or("embedded-cedar")
                .to_string(),
            policy_revision: table
                .get("policy_revision")
                .and_then(Item::as_str)
                .unwrap_or("embedded-v1")
                .to_string(),
            default_decision: table
                .get("default_decision")
                .and_then(Item::as_str)
                .unwrap_or("deny")
                .to_string(),
        }
    }

    fn from_legacy_table(table: &Table) -> Self {
        let mut config = Self::embedded_cedar();
        config.default_decision = table
            .get("default_decision")
            .and_then(Item::as_str)
            .unwrap_or("deny")
            .to_string();
        config
    }
}

fn required_str<'a>(item: &'a Item, key: &str) -> Result<&'a str> {
    item[key]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing project.{key}"))
}

fn string_array(item: &Item) -> Vec<String> {
    item.as_array()
        .map(|array| {
            array
                .iter()
                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn push_unique(items: &mut Vec<String>, value: &str) {
    if !items.iter().any(|item| item == value) {
        items.push(value.to_string());
        items.sort();
    }
}
