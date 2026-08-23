//! Cross-aggregate untyped replay feed.
//!
//! [`EventStore::load_global_after`](crate::EventStore::load_global_after) is
//! scoped to one aggregate type. The raw feed is the escape hatch for read
//! models and subscribers that need **every** event a store persisted, in
//! global commit order, regardless of aggregate type: payloads and metadata
//! come back as raw JSON for dynamic consumption. See ADR-0003 in
//! `docs/adr/` for the design decision.

use crate::event::EventEnvelope;
use std::num::NonZeroUsize;

/// Untyped persisted event.
///
/// The payload is raw JSON, and `aggregate_id` is the stored serialized-ID
/// string exactly as the backend persisted it (for JSON-serialized IDs this
/// includes the JSON quoting, e.g. `"\"counter-1\""`).
pub type RawEventEnvelope = EventEnvelope<serde_json::Value, String>;

/// Bounded cross-aggregate replay in global commit order.
///
/// Implementations return every persisted event after `sequence` — no
/// aggregate-type filter — with registered upcasters applied per event type,
/// matching typed loads. Only a bounded form exists; drive full catch-up as
/// repeated batches.
pub trait RawEventFeed {
    /// Feed-specific error type.
    type Error;

    /// Loads at most `limit` events of any aggregate type after the given
    /// global sequence number.
    fn load_raw_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<Vec<RawEventEnvelope>, Self::Error>;
}

/// Async twin of [`RawEventFeed`].
#[cfg(feature = "async")]
#[async_trait::async_trait]
pub trait AsyncRawEventFeed: Send + Sync {
    /// Feed-specific error type.
    type Error;

    /// Loads at most `limit` events of any aggregate type after the given
    /// global sequence number.
    async fn load_raw_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<Vec<RawEventEnvelope>, Self::Error>;
}
