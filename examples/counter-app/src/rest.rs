use bytes::Bytes;
use http::{Method, StatusCode};
use http_body_util::{BodyExt, StreamBody, combinators::UnsyncBoxBody};
use serde::{Deserialize, Serialize};

use crate::error::{CounterAppError, CounterAppResult};

type RestBody = UnsyncBoxBody<Bytes, std::io::Error>;
type RestResponse = http::Response<RestBody>;
type RestRequest = http::Request<wasip3::http_compat::IncomingRequestBody>;

#[derive(Deserialize)]
struct ChangeCounterRequest {
    amount: Option<i32>,
}

pub fn is_rest_request(req: &RestRequest) -> bool {
    matches!(
        req.uri().path(),
        "/api/counter/view"
            | "/api/counter/increment"
            | "/api/counter/decrement"
            | "/api/counter/reset"
    )
}

pub async fn serve(req: RestRequest) -> CounterAppResult<RestResponse> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let auth = crate::auth::CounterAuthContext::from_http_headers(req.headers());
    tracing::debug!(
        method = %method,
        path = uri.path(),
        "handling counter REST request"
    );

    match (method, uri.path()) {
        (Method::GET, "/api/counter/view") => match crate::application::get_counter_view().await {
            Ok(view) => json_response(StatusCode::OK, &view),
            Err(error) => counter_error_response(&error),
        },
        (Method::POST, "/api/counter/increment") => {
            let amount = match amount_from_request(req, 1).await {
                Ok(amount) => amount,
                Err(error) => return counter_error_response(&error),
            };
            execute(crate::domain::CounterCommand::Increment { amount }, &auth).await
        }
        (Method::POST, "/api/counter/decrement") => {
            let amount = match amount_from_request(req, 1).await {
                Ok(amount) => amount,
                Err(error) => return counter_error_response(&error),
            };
            execute(crate::domain::CounterCommand::Decrement { amount }, &auth).await
        }
        (Method::POST, "/api/counter/reset") => {
            execute(crate::domain::CounterCommand::Reset, &auth).await
        }
        (_, "/api/counter/view") => validation_error_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "GET is required for /api/counter/view",
        ),
        (_, "/api/counter/increment" | "/api/counter/decrement" | "/api/counter/reset") => {
            validation_error_response(
                StatusCode::METHOD_NOT_ALLOWED,
                "POST is required for counter command endpoints",
            )
        }
        _ => validation_error_response(StatusCode::NOT_FOUND, "unknown counter API route"),
    }
}

async fn execute(
    command: crate::domain::CounterCommand,
    auth: &crate::auth::CounterAuthContext,
) -> CounterAppResult<RestResponse> {
    match crate::application::execute_counter_command_authorized(command, auth).await {
        Ok(view) => json_response(StatusCode::OK, &view),
        Err(error) => counter_error_response(&error),
    }
}

async fn amount_from_request(req: RestRequest, default: i32) -> CounterAppResult<i32> {
    if let Some(amount) = amount_from_query(req.uri())? {
        return validate_amount(amount);
    }

    let body = req
        .into_body()
        .collect()
        .await
        .map_err(|error| {
            CounterAppError::transport(format!("failed to read request body: {error:?}"))
        })?
        .to_bytes();

    if body.is_empty() {
        return validate_amount(default);
    }

    let payload: ChangeCounterRequest = serde_json::from_slice(&body)
        .map_err(|error| CounterAppError::validation(format!("invalid JSON body: {error}")))?;
    validate_amount(payload.amount.unwrap_or(default))
}

fn amount_from_query(uri: &http::Uri) -> CounterAppResult<Option<i32>> {
    let Some(query) = uri.query() else {
        return Ok(None);
    };

    for part in query.split('&') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        if key == "amount" {
            return value.parse::<i32>().map(Some).map_err(|_| {
                CounterAppError::validation("amount query parameter must be a valid integer")
            });
        }
    }

    Ok(None)
}

fn validate_amount(amount: i32) -> CounterAppResult<i32> {
    if amount <= 0 {
        Err(CounterAppError::validation("amount must be positive"))
    } else {
        Ok(amount)
    }
}

fn json_response<T: Serialize>(status: StatusCode, value: &T) -> CounterAppResult<RestResponse> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| CounterAppError::serialization(error.to_string()))?;
    response_with_bytes(status, "application/json", Bytes::from(bytes))
}

fn counter_error_response(error: &CounterAppError) -> CounterAppResult<RestResponse> {
    log_rest_error(error, error.http_status());
    json_response(error.http_status(), &error.response_body())
}

fn validation_error_response(
    status: StatusCode,
    message: impl Into<String>,
) -> CounterAppResult<RestResponse> {
    let error = CounterAppError::validation(message);
    log_rest_error(&error, status);
    json_response(status, &error.response_body())
}

fn response_with_bytes(
    status: StatusCode,
    content_type: &'static str,
    bytes: Bytes,
) -> CounterAppResult<RestResponse> {
    let stream =
        futures::stream::once(
            async move { Ok::<_, std::io::Error>(http_body::Frame::data(bytes)) },
        );
    let body = StreamBody::new(stream).boxed_unsync();

    http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, content_type)
        .body(body)
        .map_err(|error| CounterAppError::transport(error.to_string()))
}

fn log_rest_error(error: &CounterAppError, status: StatusCode) {
    if error.is_client_error() {
        tracing::warn!(
            error = %error,
            error_code = error.public_code(),
            http_status = status.as_u16(),
            "counter REST request rejected"
        );
    } else {
        tracing::error!(
            error = %error,
            error_code = error.public_code(),
            http_status = status.as_u16(),
            "counter REST request failed"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddd_cqrs_es::{ConcurrencyError, ExpectedRevision};

    #[test]
    fn validate_amount_rejects_zero_as_validation_error() {
        let error = validate_amount(0).unwrap_err();

        assert!(matches!(error, CounterAppError::Validation { .. }));
    }

    #[test]
    fn amount_from_query_rejects_non_integer_amount() {
        let uri = "/api/counter/increment?amount=nope"
            .parse::<http::Uri>()
            .unwrap();
        let error = amount_from_query(&uri).unwrap_err();

        assert_eq!(error.http_status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn counter_error_response_uses_conflict_for_concurrency_errors() {
        let error = CounterAppError::Concurrency {
            source: ConcurrencyError::WrongExpectedRevision {
                expected: ExpectedRevision::Exact(2),
                actual: 1,
            },
        };
        let response = counter_error_response(&error).unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn validation_error_response_preserves_method_not_allowed_status() {
        let response =
            validation_error_response(StatusCode::METHOD_NOT_ALLOWED, "POST is required").unwrap();

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }
}
