use darkhorse_application::limiting::LimiterUnavailable;
use darkhorse_domain::limiting::{BudgetRule, Counter};

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
pub(super) fn unhex<const N: usize>(text: &str) -> Result<[u8; N], LimiterUnavailable> {
    if text.len() != 2 * N
        || !text
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(LimiterUnavailable);
    }
    let mut bytes = [0; N];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).map_err(|_| LimiterUnavailable)?;
    }
    Ok(bytes)
}
pub(super) fn encode(counter: Option<Counter>) -> String {
    match counter {
        None => String::new(),
        Some(c) => format!(
            "{},{},{},{},{}",
            c.rule.limit(),
            c.rule.window_ms(),
            c.used,
            c.started_ms,
            c.last_ms
        ),
    }
}
pub(super) fn decode(text: &str) -> Result<Option<Counter>, LimiterUnavailable> {
    if text.is_empty() {
        return Ok(None);
    }
    if text.len() > 96 {
        return Err(LimiterUnavailable);
    }
    let parts: Vec<_> = text.split(',').collect();
    if parts.len() != 5 {
        return Err(LimiterUnavailable);
    }
    let c = Counter {
        rule: BudgetRule::new(number(parts[0])?, number(parts[1])?)
            .map_err(|_| LimiterUnavailable)?,
        used: number(parts[2])?,
        started_ms: number(parts[3])?,
        last_ms: number(parts[4])?,
    };
    if encode(Some(c)) != text {
        return Err(LimiterUnavailable);
    }
    Ok(Some(c))
}
fn number<T: std::str::FromStr>(text: &str) -> Result<T, LimiterUnavailable> {
    text.parse().map_err(|_| LimiterUnavailable)
}

#[cfg(test)]
#[path = "../../tests/unit/redis_limiter/wire.rs"]
mod tests;
