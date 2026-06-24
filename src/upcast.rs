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
