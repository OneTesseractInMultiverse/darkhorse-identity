use darkhorse_application::DirectoryCriteria;
use darkhorse_domain::AccountStatus;
use restqs::{FieldCatalog, Parser, ParserConfig, ParserLimits, RqsValue};
use std::collections::BTreeSet;

#[derive(Debug, PartialEq, Eq)]
pub struct InvalidQuery;

pub fn parse_admin(raw: &str) -> Result<darkhorse_domain::admin_directory::Query, InvalidQuery> {
    if raw.len() > 2048 {
        return Err(InvalidQuery);
    }
    let mut seen = BTreeSet::new();
    let mut filters = url::form_urlencoded::Serializer::new(String::new());
    let mut search = String::new();
    let mut after = None;
    for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
        if !seen.insert(key.to_string()) {
            return Err(InvalidQuery);
        }
        match key.as_ref() {
            "status" | "limit" => {
                filters.append_pair(&key, &value);
            }
            "search" => search = value.trim().to_string(),
            "after" => after = Some(principal(&value)?),
            _ => return Err(InvalidQuery),
        }
    }
    let filters = parse(&filters.finish())?;
    let query = darkhorse_domain::admin_directory::Query {
        status: filters.status,
        search,
        after,
        limit: filters.limit,
    };
    query.validate().map_err(|_| InvalidQuery)?;
    Ok(query)
}
pub(crate) fn principal(
    value: &str,
) -> Result<darkhorse_domain::identity::PrincipalId, InvalidQuery> {
    let id = uuid::Uuid::parse_str(value).map_err(|_| InvalidQuery)?;
    if id.to_string() != value {
        return Err(InvalidQuery);
    }
    darkhorse_domain::identity::PrincipalId::from_u128(id.as_u128()).map_err(|_| InvalidQuery)
}

/// A transport contract example, not an authorization or persistence query.
pub fn parse(raw: &str) -> Result<DirectoryCriteria, InvalidQuery> {
    validate_parameters(raw)?;
    let catalog = FieldCatalog::new()
        .allow_text("status", "users.status")
        .map_err(|_| InvalidQuery)?;
    let config = ParserConfig::with_limits(ParserLimits {
        max_query_bytes: 2048,
        max_parameters: 3,
        max_value_bytes: 32,
        max_list_items: 1,
        max_limit: 100,
    });
    let query = Parser::with_config(&catalog, config)
        .parse(raw)
        .map_err(|_| InvalidQuery)?;
    let status = match query.filters().first().and_then(|filter| filter.value()) {
        None if query.filters().is_empty() => None,
        Some(RqsValue::Text(value)) if value == "active" => Some(AccountStatus::Active),
        Some(RqsValue::Text(value)) if value == "inactive" => Some(AccountStatus::Inactive),
        _ => return Err(InvalidQuery),
    };
    let limit = query.pagination().limit().unwrap_or(25);
    let offset = query.pagination().offset().unwrap_or(0);
    if limit == 0 || offset > 1000 {
        return Err(InvalidQuery);
    }
    Ok(DirectoryCriteria {
        status,
        limit: limit as u16,
        offset: offset as u16,
    })
}

fn validate_parameters(raw: &str) -> Result<(), InvalidQuery> {
    if raw.len() > 2048 {
        return Err(InvalidQuery);
    }
    let mut seen = BTreeSet::new();
    for (key, _) in url::form_urlencoded::parse(raw.as_bytes()) {
        if !matches!(key.as_ref(), "status" | "limit" | "skip") || !seen.insert(key.into_owned()) {
            return Err(InvalidQuery);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/directory_query.rs"]
mod tests;
