use super::*;
use darkhorse_domain::oidc::Interaction;
pub(super) fn error(error: Error) -> Response {
    let status = match error {
        Error::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::BAD_REQUEST,
    };
    (status, Json(serde_json::json!({"error":code(error)}))).into_response()
}
pub(super) fn code(error: Error) -> &'static str {
    match error {
        Error::InvalidRequest => "invalid_request",
        Error::InvalidScope => "invalid_scope",
        Error::InvalidTarget => "invalid_target",
        Error::InvalidTransaction => "invalid_transaction",
        Error::LoginRequired => "login_required",
        Error::ConsentRequired => "consent_required",
        Error::AccessDenied => "access_denied",
        Error::Unavailable => "temporarily_unavailable",
    }
}
pub(super) fn view(view: View, digest: [u8; 32]) -> Response {
    Json(serde_json::json!({"request_id":session_secret::hex(&digest),"client_name":view.client_name,"scopes":view.scopes,"resource":view.resource,"status":match view.interaction {Interaction::Login=>"login",Interaction::Consent=>"consent",Interaction::Ready=>"ready"}})).into_response()
}
fn location(target: ReturnTo, error: Error, issuer: &str) -> Result<String, Error> {
    let mut url = url::Url::parse(&target.uri).map_err(|_| Error::Unavailable)?;
    let mut query = url.query_pairs_mut();
    query.append_pair("error", code(error));
    query.append_pair("iss", issuer);
    if let Some(state) = target.state {
        query.append_pair("state", &state);
    }
    drop(query);
    Ok(url.into())
}
pub(super) fn redirect(target: ReturnTo, error_code: Error, issuer: &str) -> Response {
    match location(target, error_code, issuer) {
        Ok(uri) => Redirect::to(&uri).into_response(),
        Err(e) => error(e),
    }
}
pub(super) fn return_json(target: ReturnTo, error_code: Error, issuer: &str) -> Response {
    match location(target, error_code, issuer) {
        Ok(uri) => Json(serde_json::json!({"redirect":uri})).into_response(),
        Err(e) => error(e),
    }
}

pub(super) fn success(code: Code, issuer: &str, json: bool) -> Response {
    let Ok(mut url) = url::Url::parse(&code.target.uri) else {
        return error(Error::Unavailable);
    };
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("code", &code.value)
            .append_pair("iss", issuer);
        if let Some(state) = code.target.state {
            query.append_pair("state", &state);
        }
    }
    if json {
        Json(serde_json::json!({"redirect":url.as_str()})).into_response()
    } else {
        Redirect::to(url.as_str()).into_response()
    }
}
