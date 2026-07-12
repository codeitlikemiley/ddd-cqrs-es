use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
pub enum Preset {
    Basic,
    LeptosWasi,
    Fullstack,
    NativeApi,
    Worker,
    Custom,
}

impl Preset {
    pub const ALL: [Self; 6] = [
        Self::Basic,
        Self::LeptosWasi,
        Self::Fullstack,
        Self::NativeApi,
        Self::Worker,
        Self::Custom,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::LeptosWasi => "leptos-wasi",
            Self::Fullstack => "fullstack",
            Self::NativeApi => "native-api",
            Self::Worker => "worker",
            Self::Custom => "custom",
        }
    }
}

impl Display for Preset {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
pub enum Runtime {
    Spin,
}

impl Runtime {
    pub const ALL: [Self; 1] = [Self::Spin];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Spin => "spin",
        }
    }
}

impl Display for Runtime {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
pub enum DbBackend {
    Sqlite,
    Postgres,
    Neon,
    Supabase,
    Turso,
    Mysql,
    Redis,
}

impl DbBackend {
    pub const ALL: [Self; 7] = [
        Self::Sqlite,
        Self::Postgres,
        Self::Neon,
        Self::Supabase,
        Self::Turso,
        Self::Mysql,
        Self::Redis,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Postgres => "postgres",
            Self::Neon => "neon",
            Self::Supabase => "supabase",
            Self::Turso => "turso",
            Self::Mysql => "mysql",
            Self::Redis => "redis",
        }
    }

    pub const fn feature(self, runtime: Runtime) -> &'static str {
        match (self, runtime) {
            (Self::Sqlite, Runtime::Spin) => "spin-sqlite",
            (Self::Postgres, Runtime::Spin) => "spin-postgres",
            (Self::Neon, Runtime::Spin) => "wasi-neon",
            (Self::Supabase, Runtime::Spin) => "wasi-supabase-rpc",
            (Self::Turso, Runtime::Spin) => "wasi-libsql",
            (Self::Mysql, Runtime::Spin) => "spin-mysql",
            (Self::Redis, Runtime::Spin) => "spin-redis",
        }
    }
}

impl Display for DbBackend {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
pub enum Realtime {
    Off,
    Polling,
    Redis,
}

impl Realtime {
    pub const ALL: [Self; 3] = [Self::Off, Self::Polling, Self::Redis];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Polling => "polling",
            Self::Redis => "redis",
        }
    }
}

impl Display for Realtime {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
pub enum Transport {
    Http,
    Grpc,
    Both,
}

impl Transport {
    pub const ALL: [Self; 3] = [Self::Http, Self::Grpc, Self::Both];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Grpc => "grpc",
            Self::Both => "both",
        }
    }
}

impl Display for Transport {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
pub enum Ui {
    None,
    Leptos,
}

impl Ui {
    pub const ALL: [Self; 2] = [Self::None, Self::Leptos];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Leptos => "leptos",
        }
    }
}

impl Display for Ui {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
pub enum OAuthProviderKind {
    Google,
    Apple,
    Facebook,
}

impl OAuthProviderKind {
    pub const ALL: [Self; 3] = [Self::Google, Self::Apple, Self::Facebook];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Apple => "apple",
            Self::Facebook => "facebook",
        }
    }

    pub const fn issuer(self) -> &'static str {
        match self {
            Self::Google => "https://accounts.google.com",
            Self::Apple => "https://appleid.apple.com",
            Self::Facebook => "https://www.facebook.com",
        }
    }

    pub const fn scopes(self) -> &'static [&'static str] {
        match self {
            Self::Google => &["openid", "email", "profile"],
            Self::Apple => &["name", "email"],
            Self::Facebook => &["email", "public_profile"],
        }
    }

    pub const fn client_id_env(self) -> &'static str {
        match self {
            Self::Google => "AUTH_GOOGLE_CLIENT_ID",
            Self::Apple => "AUTH_APPLE_CLIENT_ID",
            Self::Facebook => "AUTH_FACEBOOK_CLIENT_ID",
        }
    }

    pub const fn enabled_env(self) -> &'static str {
        match self {
            Self::Google => "AUTH_GOOGLE_ENABLED",
            Self::Apple => "AUTH_APPLE_ENABLED",
            Self::Facebook => "AUTH_FACEBOOK_ENABLED",
        }
    }

    pub const fn client_secret_env(self) -> &'static str {
        match self {
            Self::Google => "AUTH_GOOGLE_CLIENT_SECRET",
            Self::Apple => "AUTH_APPLE_PRIVATE_KEY",
            Self::Facebook => "AUTH_FACEBOOK_CLIENT_SECRET",
        }
    }
}

impl Display for OAuthProviderKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppSelection {
    pub preset: Preset,
    pub runtime: Runtime,
    pub db: DbBackend,
    pub realtime: Realtime,
    pub transport: Transport,
    pub ui: Ui,
}

impl AppSelection {
    pub fn validate(self) -> anyhow::Result<()> {
        if self.preset == Preset::Basic && self.ui == Ui::Leptos {
            anyhow::bail!("preset=basic is domain-only; use preset=leptos-wasi for ui=leptos");
        }
        if self.preset == Preset::Fullstack {
            if !matches!(self.db, DbBackend::Sqlite | DbBackend::Postgres) {
                anyhow::bail!(
                    "preset=fullstack supports db=sqlite for development or db=postgres for production"
                );
            }
            if self.transport != Transport::Both {
                anyhow::bail!(
                    "preset=fullstack requires transport=both for web, REST, and gRPC surfaces"
                );
            }
            if self.ui != Ui::Leptos {
                anyhow::bail!("preset=fullstack requires ui=leptos for fullstack auth pages");
            }
        }

        Ok(())
    }
}

pub fn defaults_for_preset(preset: Preset) -> (Runtime, DbBackend, Realtime, Transport, Ui) {
    match preset {
        Preset::Basic => (
            Runtime::Spin,
            DbBackend::Sqlite,
            Realtime::Off,
            Transport::Http,
            Ui::None,
        ),
        Preset::LeptosWasi => (
            Runtime::Spin,
            DbBackend::Sqlite,
            Realtime::Off,
            Transport::Http,
            Ui::Leptos,
        ),
        Preset::Fullstack => (
            Runtime::Spin,
            DbBackend::Sqlite,
            Realtime::Off,
            Transport::Both,
            Ui::Leptos,
        ),
        Preset::NativeApi | Preset::Worker | Preset::Custom => (
            Runtime::Spin,
            DbBackend::Sqlite,
            Realtime::Off,
            Transport::Http,
            Ui::None,
        ),
    }
}

pub fn parse_model_value<T>(value: &str, label: &str) -> anyhow::Result<T>
where
    T: ValueEnum,
{
    T::from_str(value, true).map_err(|_| anyhow::anyhow!("invalid {label} value `{value}`"))
}
