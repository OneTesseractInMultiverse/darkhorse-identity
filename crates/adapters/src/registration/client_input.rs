use darkhorse_domain::{identity::*, registration::*};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClientInput {
    name: String,
    active: bool,
    #[serde(default, deserialize_with = "refresh")]
    refresh_tokens: Option<bool>,
    redirect_uris: Vec<String>,
    resource_ids: Vec<String>,
    scope_ids: Vec<String>,
    token_endpoint_auth_method: String,
}
impl ClientInput {
    pub(crate) fn complete_spec(self) -> Result<ClientSpec, RegistrationError> {
        if self.refresh_tokens.is_none() {
            return Err(RegistrationError::Invalid);
        }
        self.spec()
    }
    pub(crate) fn spec(self) -> Result<ClientSpec, RegistrationError> {
        let mut spec = ClientSpec::new(
            Label::new(&self.name)?,
            self.active,
            crate::registration::redirects(self.redirect_uris)?,
            self.resource_ids
                .iter()
                .map(|s| id(s, ResourceId::from_u128))
                .collect::<Result<_, _>>()?,
            self.scope_ids
                .iter()
                .map(|s| id(s, ScopeId::from_u128))
                .collect::<Result<_, _>>()?,
            &self.token_endpoint_auth_method,
        )?;
        spec.refresh_tokens = self.refresh_tokens.unwrap_or(false);
        Ok(spec)
    }
}
fn refresh<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<bool>, D::Error> {
    bool::deserialize(deserializer).map(Some)
}
fn id<T>(
    value: &str,
    constructor: impl FnOnce(u128) -> Result<T, darkhorse_domain::identity::InvalidIdentifier>,
) -> Result<T, RegistrationError> {
    constructor(
        uuid::Uuid::parse_str(value)
            .map_err(|_| RegistrationError::Invalid)?
            .as_u128(),
    )
    .map_err(|_| RegistrationError::Invalid)
}
#[cfg(test)]
#[path = "../../tests/unit/registration/client_input.rs"]
mod tests;
