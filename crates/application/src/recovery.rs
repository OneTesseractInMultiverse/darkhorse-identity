//! Serving and recovery transitions for a single local deployment.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Serve,
    Fence,
    Confirm,
}

pub enum Action<'a> {
    Serve,
    Begin(&'a str),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Denied;

pub fn decide(
    serving_permit: bool,
    stored_receipt: Option<&str>,
    action: Action<'_>,
) -> Result<Decision, Denied> {
    match (serving_permit, stored_receipt, action) {
        (true, None, Action::Serve) => Ok(Decision::Serve),
        (true, None, Action::Begin(_)) => Ok(Decision::Fence),
        (false, Some(stored), Action::Begin(requested)) if stored == requested => {
            Ok(Decision::Confirm)
        }
        _ => Err(Denied),
    }
}

#[cfg(test)]
#[path = "../tests/unit/recovery.rs"]
mod tests;
