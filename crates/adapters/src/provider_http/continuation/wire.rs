use super::*;
use darkhorse_domain::{
    identity::ClientId,
    oidc::{Prompt, Request},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    client: String,
    redirect: String,
    challenge: [u8; 32],
    state: Option<String>,
    nonce: Option<String>,
    scopes: Vec<String>,
    resource: Option<String>,
    prompt: u8,
    max_age: Option<u64>,
    ui_locale: Option<String>,
    created_ms: u64,
    initial_session: Option<[u8; 32]>,
    client_revision: u64,
    application_revision: u64,
    resource_policy_revision: Option<u64>,
}
pub(super) fn encode(pending: Preauthorization) -> Result<Vec<u8>, Error> {
    let r = pending.request;
    r.validate()?;
    serde_json::to_vec(&Envelope {
        client: uuid::Uuid::from_u128(r.client.as_u128()).to_string(),
        redirect: r.redirect,
        challenge: r.challenge,
        state: r.state,
        nonce: r.nonce,
        scopes: r.scopes,
        resource: r.resource,
        prompt: match r.prompt {
            Prompt::Default => 0,
            Prompt::None => 1,
            Prompt::Login => 2,
            Prompt::Consent => 3,
            Prompt::LoginConsent => 4,
        },
        max_age: r.max_age,
        ui_locale: r.ui_locale.map(|l| crate::localization::tag(l).to_owned()),
        created_ms: pending.created_ms,
        initial_session: pending.initial_session,
        client_revision: pending.client_revision,
        application_revision: pending.application_revision,
        resource_policy_revision: pending.resource_policy_revision,
    })
    .map_err(unavailable)
}
pub(super) fn decode(bytes: &[u8]) -> Result<Preauthorization, Error> {
    let value: Envelope = serde_json::from_slice(bytes).map_err(invalid)?;
    let request = Request {
        client: ClientId::from_u128(
            uuid::Uuid::parse_str(&value.client)
                .map_err(invalid)?
                .as_u128(),
        )
        .map_err(invalid)?,
        redirect: value.redirect,
        challenge: value.challenge,
        state: value.state,
        nonce: value.nonce,
        scopes: value.scopes,
        resource: value.resource,
        prompt: match value.prompt {
            0 => Prompt::Default,
            1 => Prompt::None,
            2 => Prompt::Login,
            3 => Prompt::Consent,
            4 => Prompt::LoginConsent,
            _ => return Err(Error::InvalidTransaction),
        },
        max_age: value.max_age,
        ui_locale: value
            .ui_locale
            .as_deref()
            .map(crate::localization::parse)
            .transpose()
            .map_err(invalid)?,
    };
    request.validate().map_err(invalid)?;
    Ok(Preauthorization {
        request,
        created_ms: value.created_ms,
        initial_session: value.initial_session,
        client_revision: value.client_revision,
        application_revision: value.application_revision,
        resource_policy_revision: value.resource_policy_revision,
    })
}
