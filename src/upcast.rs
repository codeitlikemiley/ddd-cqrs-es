use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Converts serialized event payloads from one schema version to another.
///
/// Upcasters operate on raw bytes so storage adapters can use JSON, MessagePack,
/// protobuf, or another encoding without coupling the core crate to that format.
/// They are load-time schema migration hooks, not Decider/Evolver logic:
/// command decisions remain in `Aggregate::handle`, and state evolution remains
/// in `Aggregate::apply`.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::EventUpcaster;
///
/// struct MySimpleUpcaster;
///
/// impl EventUpcaster for MySimpleUpcaster {
///     type Error = &'static str;
///
///     fn source_version(&self) -> u32 { 1 }
///     fn target_version(&self) -> u32 { 2 }
///
///     fn upcast(&self, raw_payload: Vec<u8>) -> Result<Vec<u8>, Self::Error> {
///         let mut upgraded = raw_payload;
///         upgraded.extend_from_slice(b"_v2");
///         Ok(upgraded)
///     }
/// }
///
/// let upcaster = MySimpleUpcaster;
/// let result = upcaster.upcast(b"old_data".to_vec()).unwrap();
/// assert_eq!(result, b"old_data_v2");
/// ```
pub trait EventUpcaster {
    /// Upcaster error.
    type Error;

    /// Source schema version.
    fn source_version(&self) -> u32;

    /// Target schema version.
    fn target_version(&self) -> u32;

    /// Converts one raw event payload into the next schema version.
    fn upcast(&self, raw_payload: Vec<u8>) -> Result<Vec<u8>, Self::Error>;
}

/// Type-erased upcaster allowing storage in homogeneous collections.
pub trait ErasedUpcaster: Send + Sync {
    /// Source schema version.
    fn source_version(&self) -> u32;

    /// Target schema version.
    fn target_version(&self) -> u32;

    /// Converts one raw event payload into the next schema version.
    fn upcast(
        &self,
        raw_payload: Vec<u8>,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>;
}

#[derive(Debug)]
struct UpcastError(String);

impl std::fmt::Display for UpcastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UpcastError {}

/// Error returned when registering a duplicate upcaster for the same source version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpcasterRegistrationError {
    /// Event type that already has an upcaster registered.
    pub event_type: String,
    /// Source schema version that is already registered.
    pub source_version: u32,
}

impl std::fmt::Display for UpcasterRegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "upcaster for `{}` at source version {} is already registered",
            self.event_type, self.source_version
        )
    }
}

impl std::error::Error for UpcasterRegistrationError {}

impl<T> ErasedUpcaster for T
where
    T: EventUpcaster + Send + Sync + 'static,
    T::Error: std::fmt::Debug + std::fmt::Display + Send + Sync + 'static,
{
    fn source_version(&self) -> u32 {
        self.source_version()
    }

    fn target_version(&self) -> u32 {
        self.target_version()
    }

    fn upcast(
        &self,
        raw_payload: Vec<u8>,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        self.upcast(raw_payload).map_err(|e| {
            Box::new(UpcastError(e.to_string())) as Box<dyn std::error::Error + Send + Sync>
        })
    }
}

/// An in-memory upcaster registry containing type-erased sequential upcaster pipelines.
#[derive(Clone, Default)]
pub struct UpcasterRegistry {
    #[allow(clippy::type_complexity)]
    upcasters: Arc<RwLock<HashMap<String, HashMap<u32, Arc<dyn ErasedUpcaster>>>>>,
}

impl UpcasterRegistry {
    /// Creates a new empty upcaster registry.
    pub fn new() -> Self {
        Self {
            upcasters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Returns `true` when no upcasters are registered for any event type.
    pub fn is_empty(&self) -> bool {
        self.upcasters
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    }

    /// Returns `true` when at least one upcaster is registered for `event_type`.
    pub fn has_upcasters(&self, event_type: &str) -> bool {
        self.upcasters
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(event_type)
    }

    /// Upcasts `payload_bytes` when needed, otherwise returns the input unchanged.
    pub fn prepare_payload(
        &self,
        event_type: &str,
        event_version: u32,
        payload_bytes: Vec<u8>,
    ) -> Result<(u32, Vec<u8>), Box<dyn std::error::Error + Send + Sync>> {
        if self.is_empty() || !self.has_upcasters(event_type) {
            Ok((event_version, payload_bytes))
        } else {
            self.upcast(event_type, event_version, payload_bytes)
        }
    }

    /// Registers an upcaster for a specific event type.
    ///
    /// Upcasters must strictly increase the version (`target_version >
    /// source_version`); a non-advancing upcaster causes [`Self::upcast`] to
    /// return an error when its source version is reached.
    pub fn register<U>(
        &self,
        event_type: impl Into<String>,
        upcaster: U,
    ) -> Result<(), UpcasterRegistrationError>
    where
        U: EventUpcaster + Send + Sync + 'static,
        U::Error: std::fmt::Debug + std::fmt::Display + Send + Sync + 'static,
    {
        let event_type = event_type.into();
        let source_version = upcaster.source_version();
        let mut map = self
            .upcasters
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let versions = map.entry(event_type.clone()).or_default();
        if versions.contains_key(&source_version) {
            return Err(UpcasterRegistrationError {
                event_type,
                source_version,
            });
        }
        versions.insert(source_version, Arc::new(upcaster));
        Ok(())
    }

    /// Automatically chains matching upcasters sequentially to upgrade the payload
    /// from the current version to the highest possible version.
    ///
    /// Every hop must strictly increase the version; an upcaster whose target
    /// version does not advance past its source version is reported as an
    /// error instead of looping forever.
    pub fn upcast(
        &self,
        event_type: &str,
        mut current_version: u32,
        mut raw_payload: Vec<u8>,
    ) -> Result<(u32, Vec<u8>), Box<dyn std::error::Error + Send + Sync>> {
        let map = self
            .upcasters
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(versions) = map.get(event_type) {
            let max_target = versions
                .values()
                .map(|upcaster| upcaster.target_version())
                .max()
                .unwrap_or(current_version);

            while let Some(upcaster) = versions.get(&current_version) {
                let target_version = upcaster.target_version();
                if target_version <= current_version {
                    return Err(Box::new(UpcastError(format!(
                        "upcaster for `{event_type}` maps version {current_version} to \
                         {target_version}, which does not advance the schema version; \
                         refusing a non-terminating upcast chain"
                    ))));
                }
                raw_payload = upcaster.upcast(raw_payload)?;
                current_version = target_version;
            }

            if current_version < max_target {
                return Err(Box::new(UpcastError(format!(
                    "no upcast path for `{event_type}` from stored version {current_version} \
                     to registered version {max_target}"
                ))));
            }
        }
        Ok((current_version, raw_payload))
    }
}

impl std::fmt::Debug for UpcasterRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpcasterRegistry").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::{EventUpcaster, UpcasterRegistry};

    struct Step {
        from: u32,
        to: u32,
    }

    impl EventUpcaster for Step {
        type Error = std::convert::Infallible;

        fn source_version(&self) -> u32 {
            self.from
        }

        fn target_version(&self) -> u32 {
            self.to
        }

        fn upcast(&self, mut raw_payload: Vec<u8>) -> Result<Vec<u8>, Self::Error> {
            raw_payload.push(self.to as u8);
            Ok(raw_payload)
        }
    }

    #[test]
    fn upcast_chains_strictly_increasing_versions() {
        let registry = UpcasterRegistry::new();
        registry.register("evt", Step { from: 1, to: 2 }).unwrap();
        registry.register("evt", Step { from: 2, to: 3 }).unwrap();

        let (version, payload) = registry.upcast("evt", 1, vec![0]).unwrap();

        assert_eq!(version, 3);
        assert_eq!(payload, vec![0, 2, 3]);
    }

    #[test]
    fn register_rejects_duplicate_source_version() {
        let registry = UpcasterRegistry::new();
        registry.register("evt", Step { from: 1, to: 2 }).unwrap();
        let error = registry
            .register("evt", Step { from: 1, to: 3 })
            .unwrap_err();
        assert_eq!(error.source_version, 1);
        assert_eq!(error.event_type, "evt");
    }

    #[test]
    fn upcast_rejects_non_advancing_upcaster_instead_of_looping() {
        let registry = UpcasterRegistry::new();
        registry.register("evt", Step { from: 2, to: 2 }).unwrap();

        let error = registry.upcast("evt", 2, Vec::new()).unwrap_err();

        assert!(error.to_string().contains("does not advance"));
    }

    #[test]
    fn upcast_rejects_version_cycle_instead_of_looping() {
        let registry = UpcasterRegistry::new();
        registry.register("evt", Step { from: 1, to: 2 }).unwrap();
        registry.register("evt", Step { from: 2, to: 1 }).unwrap();

        let error = registry.upcast("evt", 1, Vec::new()).unwrap_err();

        assert!(error.to_string().contains("does not advance"));
    }

    #[test]
    fn empty_registry_skips_upcast_path() {
        let registry = UpcasterRegistry::new();
        assert!(registry.is_empty());
        assert!(!registry.has_upcasters("evt"));

        let (version, payload) = registry.prepare_payload("evt", 1, b"raw".to_vec()).unwrap();
        assert_eq!(version, 1);
        assert_eq!(payload, b"raw");
    }

    #[test]
    fn prepare_payload_upcasts_when_registered() {
        let registry = UpcasterRegistry::new();
        registry.register("evt", Step { from: 1, to: 2 }).unwrap();

        assert!(registry.has_upcasters("evt"));
        assert!(!registry.has_upcasters("other"));

        let (version, payload) = registry.prepare_payload("evt", 1, vec![0]).unwrap();
        assert_eq!(version, 2);
        assert_eq!(payload, vec![0, 2]);
    }

    #[test]
    fn upcast_rejects_missing_migration_path() {
        let registry = UpcasterRegistry::new();
        registry.register("evt", Step { from: 2, to: 3 }).unwrap();

        let error = registry.upcast("evt", 1, Vec::new()).unwrap_err();

        assert!(error.to_string().contains("no upcast path"));
    }

    #[test]
    fn upcast_allows_stored_version_beyond_registered_chain() {
        let registry = UpcasterRegistry::new();
        registry.register("evt", Step { from: 1, to: 2 }).unwrap();

        let (version, payload) = registry.upcast("evt", 5, b"future".to_vec()).unwrap();

        assert_eq!(version, 5);
        assert_eq!(payload, b"future");
    }
}
