use darkhorse_domain::{
    authorization::CapabilitySelection,
    identity::*,
    personal_keys::{Error, Expiration, Request, Selection},
};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Creation {
    name: String,
    application_id: String,
    policy_revision: String,
    expiration: Lifetime,
    grants: Vec<RequestedGrant>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Lifetime {
    Default,
    Days { days: u16 },
    Never,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestedGrant {
    resource_id: String,
    selection: SelectionInput,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SelectionInput {
    All,
    Subset { capabilities: Vec<String> },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Revocation {
    pub key_id: String,
}
pub(super) fn identifier(value: &str) -> Result<u128, Error> {
    let uuid = uuid::Uuid::parse_str(value).map_err(|_| Error::Invalid)?;
    if uuid.is_nil() || uuid.to_string() != value {
        return Err(Error::Invalid);
    }
    Ok(uuid.as_u128())
}
pub(super) fn cursor(query: Option<&str>) -> Result<Option<u128>, Error> {
    let Some(query) = query else { return Ok(None) };
    if query.len() > 128 {
        return Err(Error::Invalid);
    }
    let mut fields = url::form_urlencoded::parse(query.as_bytes());
    let (key, value) = fields.next().ok_or(Error::Invalid)?;
    if key != "after" || fields.next().is_some() {
        return Err(Error::Invalid);
    }
    identifier(&value).map(Some)
}
pub(super) fn request(input: Creation) -> Result<Request, Error> {
    let revision = input
        .policy_revision
        .parse::<u64>()
        .map_err(|_| Error::Invalid)?;
    if revision.to_string() != input.policy_revision {
        return Err(Error::Invalid);
    }
    let expiration = match input.expiration {
        Lifetime::Default => Expiration::Default,
        Lifetime::Days { days } => Expiration::Days(days),
        Lifetime::Never => Expiration::Never,
    };
    let grants = input
        .grants
        .into_iter()
        .map(grant)
        .collect::<Result<Vec<_>, _>>()?;
    Request::new(
        &input.name,
        ApplicationId::from_u128(identifier(&input.application_id)?).map_err(|_| Error::Invalid)?,
        revision,
        expiration,
        grants,
    )
}
fn grant(input: RequestedGrant) -> Result<Selection, Error> {
    let selection = match input.selection {
        SelectionInput::All => CapabilitySelection::All,
        SelectionInput::Subset { capabilities } => {
            let caps = capabilities
                .iter()
                .map(|id| CapabilityId::from_u128(identifier(id)?).map_err(|_| Error::Invalid))
                .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
            if caps.len() != capabilities.len() {
                return Err(Error::Invalid);
            }
            CapabilitySelection::Subset(caps)
        }
    };
    Ok(Selection {
        resource: ResourceId::from_u128(identifier(&input.resource_id)?)
            .map_err(|_| Error::Invalid)?,
        capabilities: selection,
    })
}
#[cfg(test)]
#[path = "../../tests/unit/personal_keys_http/input.rs"]
mod tests;
