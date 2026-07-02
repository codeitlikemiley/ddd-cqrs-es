---
title: Leptos Server APIs
description: Wire Leptos server functions, REST endpoints, gRPC calls, and WASI HTTP routing around the CQRS command bus.
---

# Leptos Server APIs

## 5. Leptos SSR, REST, and gRPC Integration

Leptos server functions (`#[server]`) bridge the browser UI with backend
commands. The counter app also exposes explicit JSON REST endpoints and, on
Spin, a gRPC service through the same WASI HTTP trigger. All three command
surfaces call the same application service so persistence, projection catch-up,
and Redis realtime publishing stay consistent.

### 5.1 Server Function Definitions (`src/app.rs`)

Here is how our server functions are constructed inside `/Users/uriah/Code/ddd/examples/counter-app/src/app.rs`. Notice how they isolate SSR execution from client-side WASM hydration compilation:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventLogDto {
    pub sequence: u64,
    pub event_type: String,
    pub revision: u64,
    pub payload: String,
    pub recorded_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CounterViewDto {
    pub count: i32,
    pub latest_events: Vec<EventLogDto>,
    pub last_sequence: u64,
    pub realtime_enabled: bool,
}

#[server(prefix = "/api")]
pub async fn get_counter_view() -> Result<CounterViewDto, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        get_counter_view_db().await
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}

#[server(prefix = "/api")]
pub async fn increment_count(amount: i32) -> Result<CounterViewDto, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        if amount <= 0 {
            return Err(server_fn_error(crate::error::CounterAppError::validation(
                "amount must be positive",
            )));
        }
        run_cqrs_command(crate::domain::CounterCommand::Increment { amount }).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = amount;
        unreachable!()
    }
}

#[server(prefix = "/api")]
pub async fn decrement_count(amount: i32) -> Result<CounterViewDto, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        if amount <= 0 {
            return Err(server_fn_error(crate::error::CounterAppError::validation(
                "amount must be positive",
            )));
        }
        run_cqrs_command(crate::domain::CounterCommand::Decrement { amount }).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = amount;
        unreachable!()
    }
}

#[server(prefix = "/api")]
pub async fn reset_count() -> Result<CounterViewDto, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        run_cqrs_command(crate::domain::CounterCommand::Reset).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}
```

### 5.2 Unified Server-Side Command Execution (`src/application.rs`)

Behind the scenes on the server, the application layer initializes the event
store, executes the command within aggregate consistency boundaries through the
repository, publishes realtime notifications, and advances the projection
runner. The Leptos server functions, REST routes, and gRPC service all call this
same function:

```rust
#[cfg(feature = "ssr")]
pub async fn execute_counter_command(
    command: crate::domain::CounterCommand,
) -> crate::error::CounterAppResult<CounterViewDto> {
    use crate::domain::{Counter, CounterId};
    use crate::error::CounterAppError;
    use crate::store::MultiBackendEventStore;
    use ddd_cqrs_es::AsyncRepository;

    let event_store = MultiBackendEventStore::<Counter>::new();
    let repository = AsyncRepository::new(event_store);
    let aggregate_id = CounterId("global".to_string());

    let (loaded, committed_events) = repository
        .execute_returning_state(
            &aggregate_id,
            command,
            ddd_cqrs_es::Metadata::default(),
        )
        .await
        .map_err(CounterAppError::from_repository_error)?;

    let mut view = get_counter_view_db().await?;
    view.count = loaded.state.value;
    if let Some(last_sequence) = committed_events.last().and_then(|event| event.sequence) {
        view.last_sequence = last_sequence;
    }
    if let Err(error) = crate::store::publish_counter_realtime(&view).await {
        tracing::error!(error = %error, error_code = error.public_code());
    }
    if let Err(error) = crate::store::catch_up_counter_projection().await {
        tracing::error!(error = %error, error_code = error.public_code());
    }

    Ok(view)
}
```

The counter app keeps typed errors until the transport boundary. REST serializes
`{"error":{"code":"...","message":"..."}}`, gRPC maps the same error to a
`tonic::Code`, and server functions convert it to `ServerFnError` only after
logging through `tracing`. See [Error Handling and Transport Mapping](../production/error-handling) for the complete production guide.

### 5.3 Curlable REST and Spin gRPC APIs

The UI server functions are framework-owned endpoints. For integration checks,
the app exposes these explicit JSON REST routes:

```bash
curl -sS http://127.0.0.1:3000/api/counter/view
curl -sS -X POST -H 'content-type: application/json' \
  -d '{"amount":1}' \
  http://127.0.0.1:3000/api/counter/increment
curl -sS -X POST 'http://127.0.0.1:3000/api/counter/decrement?amount=1'
curl -sS -X POST http://127.0.0.1:3000/api/counter/reset
```

Spin gRPC uses `proto/counter.proto` and is served through the Spin HTTP
trigger. Run with `transport=both` to keep the browser UI, REST APIs, SSE, and
gRPC active together:

```bash
RUST_LOG=info,counter_app=debug make spin db=sqlite transport=both realtime=redis
grpcurl -plaintext \
  -import-path proto \
  -proto counter.proto \
  -d '{"amount":1}' \
  localhost:3000 \
  counter.v1.CounterService/Increment
```

`transport=grpc` serves only gRPC endpoints. `transport=both` serves HTTP UI,
REST, SSE, and gRPC. Wasmtime currently fails fast for `transport=grpc` and
`transport=both`.

### 5.4 WASI HTTP Routing (`src/server.rs`)

The WASI HTTP router handles transport-specific routes before handing normal UI
and server-function traffic to `leptos_wasi::Handler`. Keep this order:

1. Spin gRPC route detection.
2. `transport=grpc` HTTP guard.
3. Explicit JSON REST counter routes.
4. `/api/counter/stream` SSE realtime route.
5. Leptos static-file, UI, and server-function handler.

```rust
use leptos_wasi::prelude::Handler;
use wasip3::http::types::{Request, Response, ErrorCode};
use crate::app::{shell, App, GetCounterView, IncrementCount, DecrementCount, ResetCount};

struct LeptosServer;

impl wasip3::exports::http::handler::Guest for LeptosServer {
    async fn handle(request: Request) -> Result<Response, ErrorCode> {
        let _ = init_wasip3_spawner();
        let req = wasip3::http_compat::http_from_wasi_request(request)?;
        let request_path = req.uri().path().to_string();

        #[cfg(all(feature = "spin-grpc", runtime_spin))]
        if crate::grpc::is_grpc_request(&req) {
            return crate::grpc::serve(req).await;
        }

        if transport_mode() == "grpc" {
            return plain_text_response(
                http::StatusCode::NOT_FOUND,
                "This component is running with transport=grpc. Use the gRPC service endpoint.",
            );
        }

        if crate::rest::is_rest_request(&req) {
            let response = crate::rest::serve(req)
                .await
                .map_err(|_| ErrorCode::InternalError(None))?;
            return wasip3::http_compat::http_into_wasi_response(response);
        }

        if request_path == "/api/counter/stream" {
            let response = crate::store::counter_stream_response(&req)
                .await
                .map_err(|_| ErrorCode::InternalError(None))?;
            return wasip3::http_compat::http_into_wasi_response(response);
        }

        let conf = get_configuration(None).unwrap();
        let leptos_options = conf.leptos_options;

        let wasi_res = Handler::build(req).await
            .map_err(|e| ErrorCode::InternalError(None))?
            .static_files_handler("/pkg", serve_static_files)
            .with_server_fn::<GetCounterView>()
            .with_server_fn::<IncrementCount>()
            .with_server_fn::<DecrementCount>()
            .with_server_fn::<ResetCount>()
            .generate_routes(App)
            .handle_with_context(move || shell(leptos_options.clone()), || {})
            .await
            .map_err(|e| ErrorCode::InternalError(None))?;

        Ok(wasi_res)
    }
}
```

---

