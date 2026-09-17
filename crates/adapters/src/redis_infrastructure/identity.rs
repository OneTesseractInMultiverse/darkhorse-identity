use super::{Identity, ProbeFailure, Role};

pub(crate) fn validate(
    role: Role,
    pong: &str,
    server: &str,
    memory: &str,
    replication: &str,
) -> Result<Identity, ProbeFailure> {
    if pong != "PONG"
        || field(server, "redis_mode")? != "standalone"
        || field(replication, "role")? != "master"
        || field(memory, "maxmemory_policy")? != role.eviction_policy()
    {
        return Err(ProbeFailure::UnsafeConfiguration);
    }
    let run_id = field(server, "run_id")?;
    if run_id.len() != 40
        || !run_id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(ProbeFailure::UnsafeConfiguration);
    }
    let memory_limit_bytes = field(memory, "maxmemory")?
        .parse::<u64>()
        .map_err(|_| ProbeFailure::UnsafeConfiguration)?;
    let used_memory_bytes = field(memory, "used_memory")?
        .parse::<u64>()
        .map_err(|_| ProbeFailure::UnsafeConfiguration)?;
    if memory_limit_bytes == 0 || (role == Role::Limiter && used_memory_bytes > memory_limit_bytes)
    {
        return Err(ProbeFailure::UnsafeConfiguration);
    }
    Ok(Identity {
        run_id: run_id.to_owned(),
        memory_limit_bytes,
        used_memory_bytes,
    })
}
pub(crate) fn field<'a>(text: &'a str, name: &str) -> Result<&'a str, ProbeFailure> {
    if text.len() > 16384 {
        return Err(ProbeFailure::UnsafeConfiguration);
    }
    let mut values = text
        .lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(key, _)| *key == name)
        .map(|(_, value)| value);
    let value = values.next().ok_or(ProbeFailure::UnsafeConfiguration)?;
    if values.next().is_some() {
        return Err(ProbeFailure::UnsafeConfiguration);
    }
    Ok(value)
}
#[cfg(test)]
#[path = "../../tests/unit/redis_infrastructure/identity.rs"]
mod tests;
