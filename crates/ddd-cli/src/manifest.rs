use crate::model::{
    parse_model_value, AppSelection, DbBackend, Preset, Realtime, Runtime, Transport, Ui,
};
use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use toml_edit::{value, Array, DocumentMut, Item, Table};

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
    pub domains: Vec<DomainRecord>,
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

        Self {
            name: name.into(),
            preset: selection.preset,
            runtime: selection.runtime,
            db: selection.db,
            realtime: selection.realtime,
            transport: selection.transport,
            ui: selection.ui,
            capabilities,
            domains: vec![domain],
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
            domains,
        })
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
