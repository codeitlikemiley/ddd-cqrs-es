use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Converts serialized event payloads from one schema version to another.
///
/// Upcasters operate on raw bytes so storage adapters can use JSON, MessagePack,
/// protobuf, or another encoding without coupling the core crate to that format.
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

    /// Registers an upcaster for a specific event type.
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
                    raw_payload = upcaster.upcast(raw_payload)?;
                    current_version = upcaster.target_version();
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
