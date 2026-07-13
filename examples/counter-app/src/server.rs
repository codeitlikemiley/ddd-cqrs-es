use leptos::config::get_configuration;
use leptos_wasi::wasip3::prelude::{Handler, HandlerConfig, init_wasip3_spawner};
use wasip3::http::types::{ErrorCode, Request, Response};

use crate::app::{App, DecrementCount, GetCounterView, IncrementCount, ResetCount, shell};

struct LeptosServer;

impl wasip3::exports::http::handler::Guest for LeptosServer {
    async fn handle(request: Request) -> Result<Response, ErrorCode> {
        init_wasip3_spawner().map_err(internal_error)?;

        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        // Convert the WASI request to http::Request before storage work so
        // static assets do not trigger remote schema checks.
        let req = wasip3::http_compat::http_from_wasi_request(request)?;
        let request_path = req.uri().path().to_string();
        tracing::debug!(
            method = %req.method(),
            path = %request_path,
            transport = %transport_mode(),
            "handling counter request"
        );

        // Store-level initialization is guarded by an async lock, so concurrent
        // first requests do not run migrations more than once.
        if !request_path.starts_with("/pkg/")
            && let Err(e) = crate::store::initialize_schema_async().await
        {
            tracing::error!(
                error = %e,
                path = request_path,
                "failed to execute boot schema migrations"
            );
            return Err(ErrorCode::InternalError(None));
        }

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
            let response = crate::rest::serve(req).await.map_err(|error| {
                tracing::error!(
                    error = %error,
                    error_code = error.public_code(),
                    "failed to build counter REST response"
                );
                ErrorCode::InternalError(None)
            })?;
            return wasip3::http_compat::http_into_wasi_response(response);
        }

        if request_path == "/api/counter/stream" {
            let response = crate::store::counter_stream_response(&req)
                .await
                .map_err(|error| {
                    tracing::error!(
                        error = %error,
                        error_code = error.public_code(),
                        "failed to build counter stream response"
                    );
                    ErrorCode::InternalError(None)
                })?;
            return wasip3::http_compat::http_into_wasi_response(response);
        }

        let conf = get_configuration(None).map_err(internal_error)?;
        let leptos_options = conf.leptos_options;

        let handler = Handler::build_with_config(
            req,
            HandlerConfig::default().with_max_request_body_size(256 * 1024),
        )
        .await
        .map_err(internal_error)?;
        let handler = handler
            .static_files_handler("/pkg", serve_static_files)
            .map_err(internal_error)?
            .with_server_fn::<GetCounterView>()
            .with_server_fn::<IncrementCount>()
            .with_server_fn::<DecrementCount>()
            .with_server_fn::<ResetCount>()
            .generate_routes(App)
            .map_err(internal_error)?;
        let wasi_res = handler
            .handle_with_context(move || shell(leptos_options.clone()), || {})
            .await
            .map_err(internal_error)?;

        Ok(wasi_res)
    }
}

fn internal_error(error: impl std::fmt::Debug) -> ErrorCode {
    tracing::error!(error = ?error, "counter request failed");
    ErrorCode::InternalError(None)
}

fn serve_static_files(path: String) -> Option<leptos_wasi::response::Body> {
    use std::fs;
    let path = path.strip_prefix("/").unwrap_or(&path);
    // Wasmtime mounts site directory at root, so look at /path directly
    let file_path = format!("/{}", path);
    tracing::debug!(file_path, "serving static file");

    if let Ok(bytes) = fs::read(&file_path) {
        Some(leptos_wasi::response::Body::Sync(bytes.into()))
    } else {
        tracing::warn!(file_path, "could not read static file");
        None
    }
}

fn transport_mode() -> String {
    std::env::var("TRANSPORT_MODE").unwrap_or_else(|_| "http".to_string())
}

fn plain_text_response(
    status: http::StatusCode,
    message: &'static str,
) -> Result<Response, ErrorCode> {
    use http_body_util::BodyExt;

    let stream = futures::stream::once(async move {
        Ok::<_, std::io::Error>(http_body::Frame::data(bytes::Bytes::from_static(
            message.as_bytes(),
        )))
    });
    let body = http_body_util::StreamBody::new(stream).boxed_unsync();
    let response = http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(body)
        .map_err(|error| {
            tracing::error!(
                error = %error,
                "failed to build plain text response"
            );
            ErrorCode::InternalError(None)
        })?;
    wasip3::http_compat::http_into_wasi_response(response)
}

// Export the server for standard WASIp3 http trigger
wasip3::http::service::export!(LeptosServer);
