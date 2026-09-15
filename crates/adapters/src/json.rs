use axum::{
    Json,
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Serialize, de::DeserializeOwned};

pub struct SafeJson<T>(pub T);

#[derive(Serialize)]
struct Rejection {
    error: &'static str,
}

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for SafeJson<T> {
    type Rejection = Response;
    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|rejection| invalid_request(rejection.status()))
    }
}

fn invalid_request(status: StatusCode) -> Response {
    let status = match status {
        StatusCode::PAYLOAD_TOO_LARGE | StatusCode::UNSUPPORTED_MEDIA_TYPE => status,
        _ => StatusCode::BAD_REQUEST,
    };
    (
        status,
        Json(Rejection {
            error: "invalid_request",
        }),
    )
        .into_response()
}

#[cfg(test)]
#[path = "../tests/unit/json.rs"]
mod tests;
