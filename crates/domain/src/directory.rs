//! Directory validation and account transitions from explicit facts.
use crate::AccountStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    email: String,
    email_key: String,
    first_name: String,
    last_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryError {
    Email,
    Name,
    Password,
    LastAdministrator,
    CounterExhausted,
}

impl Profile {
    pub fn new(email: &str, first_name: &str, last_name: &str) -> Result<Self, DirectoryError> {
        let email = validate_email(email.trim())?;
        Ok(Self {
            email: email.to_owned(),
            email_key: email.to_ascii_lowercase(),
            first_name: validate_name(first_name)?,
            last_name: validate_name(last_name)?,
        })
    }
    pub fn email(&self) -> &str {
        &self.email
    }
    pub fn email_key(&self) -> &str {
        &self.email_key
    }
    pub fn first_name(&self) -> &str {
        &self.first_name
    }
    pub fn last_name(&self) -> &str {
        &self.last_name
    }
}

fn validate_email(email: &str) -> Result<&str, DirectoryError> {
    let (local, domain) = email.split_once('@').ok_or(DirectoryError::Email)?;
    if !email.is_ascii()
        || email.len() > 254
        || local.is_empty()
        || local.len() > 64
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || !local
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&c))
        || !domain.contains('.')
        || !domain.split('.').all(valid_domain_label)
    {
        return Err(DirectoryError::Email);
    }
    Ok(email)
}

fn valid_domain_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
}

fn validate_name(value: &str) -> Result<String, DirectoryError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 100 || value.chars().any(char::is_control) {
        return Err(DirectoryError::Name);
    }
    Ok(value.to_owned())
}

pub fn validate_password(password: &str) -> Result<(), DirectoryError> {
    if !(15..=128).contains(&password.chars().count()) || password.chars().any(char::is_control) {
        return Err(DirectoryError::Password);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountSnapshot {
    pub status: AccountStatus,
    pub credential_epoch: u64,
    pub revision: u64,
    pub eligible_administrator: bool,
    pub eligible_administrators: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountAction {
    SetStatus(AccountStatus),
    RevokeAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountChange {
    pub status: AccountStatus,
    pub credential_epoch: u64,
    pub revision: u64,
}

pub fn plan_change(
    snapshot: AccountSnapshot,
    action: AccountAction,
) -> Result<Option<AccountChange>, DirectoryError> {
    if action == AccountAction::SetStatus(snapshot.status) {
        return Ok(None);
    }
    let status = match action {
        AccountAction::SetStatus(status) => status,
        AccountAction::RevokeAll => snapshot.status,
    };
    if status == AccountStatus::Inactive
        && snapshot.status == AccountStatus::Active
        && snapshot.eligible_administrator
        && snapshot.eligible_administrators <= 1
    {
        return Err(DirectoryError::LastAdministrator);
    }
    let invalidate = action == AccountAction::RevokeAll || status == AccountStatus::Inactive;
    let credential_epoch = if invalidate {
        advance_counter(snapshot.credential_epoch)?
    } else {
        snapshot.credential_epoch
    };
    Ok(Some(AccountChange {
        status,
        credential_epoch,
        revision: advance_counter(snapshot.revision)?,
    }))
}

fn advance_counter(value: u64) -> Result<u64, DirectoryError> {
    value
        .checked_add(1)
        .filter(|&next| next <= i64::MAX as u64)
        .ok_or(DirectoryError::CounterExhausted)
}

#[cfg(test)]
#[path = "../tests/unit/directory.rs"]
mod tests;
