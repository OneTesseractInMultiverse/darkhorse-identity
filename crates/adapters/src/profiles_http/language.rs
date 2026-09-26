use super::*;
use darkhorse_domain::localization::Locale;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    revision: String,
    #[serde(deserialize_with = "selection")]
    locale: Option<Locale>,
}
fn selection<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Option<Locale>, D::Error> {
    let value = Option::<String>::deserialize(de)?;
    value
        .as_deref()
        .map(crate::localization::parse)
        .transpose()
        .map_err(|_| serde::de::Error::custom("Invalid language selection."))
}
pub(super) async fn update<S: Store>(
    State(store): State<Arc<S>>,
    headers: HeaderMap,
    SafeJson(input): SafeJson<Input>,
) -> Response {
    let parsed = actor(&headers).and_then(|actor| {
        Ok((
            actor,
            crate::admin_directory_http::counter(&input.revision).map_err(|_| Error::Invalid)?,
        ))
    });
    let (actor, expected) = match parsed {
        Ok(v) => v,
        Err(e) => return failure(e),
    };
    respond(store.update_language(actor, expected, input.locale).await)
}
