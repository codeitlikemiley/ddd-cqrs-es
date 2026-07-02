---
title: 5.1. Error Handling and Transport Mapping
description: Preserve typed domain and infrastructure errors, then map them safely to REST, Leptos server functions, Spin gRPC, and tracing.
---

Production error handling has two jobs:

1. Keep enough typed context inside the application to make correct decisions.
2. Return stable, safe transport errors to clients without leaking adapter or database internals.

`ddd_cqrs_es` gives you typed library errors at the repository and event-store layers. Your application should add one application error boundary on top of those library errors, then map that boundary into HTTP, gRPC, server functions, logs, and metrics.

## Layered Error Model

Keep each layer responsible for its own errors:

| Layer | Owns | Error shape |
| :--- | :--- | :--- |
| Domain | Business invariants inside `Aggregate::handle` | Aggregate-specific error type, such as `BankAccountError` or `String` in small examples |
| Repository | Command execution and optimistic concurrency | `RepositoryError<DomainError, EventStoreError>` |
| Event store | Persistence, serialization, connection, and backend failures | `EventStoreError` |
| Application | Product-facing operation failure | App-specific error, such as `CounterAppError` |
| Transport | Protocol response shape | REST status/body, `ServerFnError`, `tonic::Status` |

Do not convert repository or store errors to `String` in the shared application service. That erases the distinction between validation, concurrency, unavailable storage, serialization, and internal backend failures.

## Library Errors

Repository command paths return `RepositoryError`:

```rust
pub enum RepositoryError<DomainError, StoreError = EventStoreError> {
    Domain(DomainError),
    Concurrency(ConcurrencyError),
    Store(StoreError),
}
```

The standard `EventStoreError` preserves broad infrastructure categories:

```rust
pub enum EventStoreError {
    Concurrency(ConcurrencyError),
    Serialization(String),
    Deserialization(String),
    Connection(String),
    Poisoned,
    Backend(String),
    Unknown(String),
    // variants with preserved source context omitted
}
```

Use these variants directly when deciding status codes. Avoid parsing display strings such as `"connection error: ..."` or `"event store backend error: ..."`.

## Application Boundary Error

Apps should define a single error type close to the application service. The counter app uses `CounterAppError` for all Leptos server-function, REST, and Spin gRPC command paths.

```rust
#[derive(Debug, thiserror::Error)]
pub enum CounterAppError {
    #[error("validation error: {message}")]
    Validation { message: String },
    #[error("domain error: {message}")]
    Domain { message: String },
    #[error("concurrency error: {source}")]
    Concurrency { source: ConcurrencyError },
    #[error("event store error: {source}")]
    Store { source: EventStoreError },
    #[error("read model error: {message}")]
    ReadModel { message: String },
    #[error("projection error: {message}")]
    Projection { message: String },
    #[error("realtime error: {message}")]
    Realtime { message: String },
    #[error("configuration error: {message}")]
    Configuration { message: String },
}
```

The important conversion is from `RepositoryError` into the app boundary error:

```rust
impl CounterAppError {
    pub fn from_repository_error(
        error: RepositoryError<String, EventStoreError>,
    ) -> CounterAppError {
        match error {
            RepositoryError::Domain(message) => CounterAppError::Domain { message },
            RepositoryError::Concurrency(source) => CounterAppError::Concurrency { source },
            RepositoryError::Store(source) => CounterAppError::Store { source },
        }
    }
}
```

That preserves classification while still letting the application choose public messages, log levels, and transport status codes in one place.

## Shared Command Service

All command transports should call the same application service. The counter app command service uses `AsyncRepository::execute_returning_state`, retries expected write conflicts, and returns `CounterAppResult<CounterViewDto>`.

```rust
pub async fn execute_counter_command(
    command: CounterCommand,
) -> CounterAppResult<CounterViewDto> {
    let event_store = MultiBackendEventStore::<Counter>::new();
    let repository = AsyncRepository::new(event_store);
    let aggregate_id = CounterId("global".to_string());

    let (loaded, committed_events) = match repository
        .execute_returning_state(
            &aggregate_id,
            command,
            Metadata::default(),
        )
        .await
    {
        Ok(outcome) => outcome,
        Err(error) => return Err(CounterAppError::from_repository_error(error)),
    };

    let mut view = get_counter_view().await?;
    view.count = loaded.state.value;
    if let Some(last_sequence) = committed_events.last().and_then(|event| event.sequence) {
        view.last_sequence = last_sequence;
    }

    Ok(view)
}
```

If a command has already committed and a non-critical notification or projection catch-up fails afterward, do not report the command as failed unless your product requires synchronous projection consistency. Log the failure with enough context and let clients recover from durable event replay.

## Transport Mapping

Use one mapping table for every public transport:

| App error | REST | gRPC | Server function | Log level |
| :--- | :--- | :--- | :--- | :--- |
| `Validation` | `400 Bad Request` | `InvalidArgument` | Public validation message | `warn` |
| `Domain` | `400 Bad Request` or domain-specific `409` | `InvalidArgument` | Public domain message | `warn` |
| `Concurrency` | `409 Conflict` | `Aborted` | Retry-safe conflict message | `warn` |
| `Store(Connection)` | `503 Service Unavailable` | `Unavailable` | Generic storage unavailable message | `error` |
| `Configuration` | `503 Service Unavailable` | `Unavailable` | Generic configuration message | `error` |
| `Store`, `ReadModel`, `Projection`, `Realtime`, `Serialization`, `Transport` | `500 Internal Server Error` | `Internal` | Generic failure message | `error` |

Public messages should be stable and safe. Internal adapter messages, database URLs, SQL fragments, Redis command details, and serialized payloads belong in logs, not in client responses.

## REST JSON Errors

REST endpoints should return structured JSON with a stable code:

```json
{
  "error": {
    "code": "validation",
    "message": "amount must be positive"
  }
}
```

The counter app exposes explicit JSON REST routes:

```bash
curl -sS http://127.0.0.1:3000/api/counter/view
curl -sS -X POST -H 'content-type: application/json' \
  -d '{"amount":1}' \
  http://127.0.0.1:3000/api/counter/increment
curl -sS -X POST 'http://127.0.0.1:3000/api/counter/decrement?amount=1'
curl -sS -X POST http://127.0.0.1:3000/api/counter/reset
```

Validation proof:

```bash
curl -i -sS -X POST -H 'content-type: application/json' \
  -d '{"amount":0}' \
  http://127.0.0.1:3000/api/counter/increment
```

Expected status is `400`, and the body contains `error.code = "validation"`.

## Leptos Server Functions

Leptos server functions are framework-owned endpoints. Keep them thin:

```rust
#[server(prefix = "/api")]
pub async fn increment_count(amount: i32) -> Result<CounterViewDto, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        if amount <= 0 {
            return Err(CounterAppError::validation("amount must be positive")
                .server_fn_error());
        }

        crate::application::execute_counter_command(
            CounterCommand::Increment { amount },
        )
        .await
        .map_err(|error| error.server_fn_error())
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = amount;
        unreachable!()
    }
}
```

Server functions cannot carry the same protocol status shape as REST or gRPC. Log the typed error before converting it to `ServerFnError`, and return only the public message to the browser.

## Spin gRPC

Spin gRPC is served through the HTTP trigger. There is no separate gRPC trigger. Enable it with `transport=grpc` or `transport=both`:

```bash
make spin db=sqlite transport=both realtime=redis
```

Then call the service with `grpcurl` from `examples/counter-app`:

```bash
grpcurl -plaintext \
  -import-path proto \
  -proto counter.proto \
  -d '{"amount":1}' \
  localhost:3000 \
  counter.v1.CounterService/Increment
```

Validation proof:

```bash
grpcurl -plaintext \
  -import-path proto \
  -proto counter.proto \
  -d '{"amount":0}' \
  localhost:3000 \
  counter.v1.CounterService/Increment
```

Expected result is a gRPC `InvalidArgument` error with the public message `amount must be positive`.

Wasmtime currently supports the HTTP transport only. `make wasmtime ... transport=grpc` and `make wasmtime ... transport=both` fail fast with a Spin-only transport message.

## Tracing and Logs

Initialize `tracing_subscriber` once at the runtime entrypoint, then use structured logs at the application and transport boundaries:

```rust
tracing::error!(
    error = %error,
    error_code = error.public_code(),
    "counter REST request failed"
);
```

Do not hold a `tracing::Span::entered()` guard across `.await` in Leptos server-function futures. The guard is not `Send`, and server functions require `Send` futures. Prefer structured events, or use instrumentation patterns that do not keep an entered guard alive across `.await`.

The counter app Makefile forwards `RUST_LOG` into Spin and Wasmtime when it is set:

```bash
RUST_LOG=info,counter_app=debug make spin db=sqlite transport=both realtime=redis
```

Use this when proving REST, gRPC, SSE, and Redis-trigger behavior from local terminals.

## Tests and Verification

Error handling tests should exercise real error values, not fake service stubs:

- Construct `RepositoryError::Domain`, `RepositoryError::Concurrency`, and `RepositoryError::Store(EventStoreError::Connection)` values.
- Assert REST status and JSON code/message mapping.
- Assert gRPC `tonic::Code` mapping.
- Assert server-function conversion uses the public message.
- Compile the runtime combinations that own the transport surface.

Counter app checks:

```bash
cargo test --manifest-path examples/counter-app/Cargo.toml \
  --no-default-features \
  --features ssr,sqlite \
  --lib

cargo test --manifest-path examples/counter-app/Cargo.toml \
  --no-default-features \
  --features ssr,sqlite,spin-grpc \
  --lib

WASI_RUNTIME=spin DATABASE_BACKEND=sqlite REALTIME_BACKEND=redis \
  TRANSPORT_MODE=both LEPTOS_OUTPUT_NAME=counter_app \
  cargo build --manifest-path examples/counter-app/Cargo.toml \
  --lib --target wasm32-wasip2 --release \
  --no-default-features \
  --features ssr,sqlite,spin-redis,spin-grpc
```

Docs checks:

```bash
bash scripts/verify-docs.sh
bash scripts/verify-docs-rust.sh
```
