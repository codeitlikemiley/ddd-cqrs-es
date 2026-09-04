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
    upcasters: Arc<RwLock<HashMap<String, Vec<Arc<dyn ErasedUpcaster>>>>>,
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
    pub fn register<U>(&self, event_type: impl Into<String>, upcaster: U)
    where
        U: EventUpcaster + Send + Sync + 'static,
        U::Error: std::fmt::Debug + std::fmt::Display + Send + Sync + 'static,
    {
        let mut map = self
            .upcasters
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.entry(event_type.into())
            .or_default()
            .push(Arc::new(upcaster));
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
        if let Some(list) = map.get(event_type) {
            loop {
                // Find an upcaster that starts from current_version
                let matching = list.iter().find(|u| u.source_version() == current_version);

                if let Some(upcaster) = matching {
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
                } else {
                    break;
                }
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
        registry.register("evt", Step { from: 1, to: 2 });
        registry.register("evt", Step { from: 2, to: 3 });

        let (version, payload) = registry.upcast("evt", 1, vec![0]).unwrap();

        assert_eq!(version, 3);
        assert_eq!(payload, vec![0, 2, 3]);
    }

    #[test]
    fn upcast_rejects_non_advancing_upcaster_instead_of_looping() {
        let registry = UpcasterRegistry::new();
        registry.register("evt", Step { from: 2, to: 2 });

        let error = registry.upcast("evt", 2, Vec::new()).unwrap_err();

        assert!(error.to_string().contains("does not advance"));
    }

    #[test]
    fn upcast_rejects_version_cycle_instead_of_looping() {
        let registry = UpcasterRegistry::new();
        registry.register("evt", Step { from: 1, to: 2 });
        registry.register("evt", Step { from: 2, to: 1 });

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
        registry.register("evt", Step { from: 1, to: 2 });

        assert!(registry.has_upcasters("evt"));
        assert!(!registry.has_upcasters("other"));

        let (version, payload) = registry.prepare_payload("evt", 1, vec![0]).unwrap();
        assert_eq!(version, 2);
        assert_eq!(payload, vec![0, 2]);
    }
}
