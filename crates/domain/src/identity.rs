//! Internal identifiers are distinct from protocol strings and credentials.

use std::num::NonZeroU128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidIdentifier;

macro_rules! identifier {
    ($($name:ident),+ $(,)?) => { $(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(NonZeroU128);

        impl $name {
            pub fn from_u128(value: u128) -> Result<Self, InvalidIdentifier> {
                NonZeroU128::new(value).map(Self).ok_or(InvalidIdentifier)
            }

            pub const fn as_u128(self) -> u128 { self.0.get() }
        }
    )+ };
}

identifier!(
    AssetId,
    OperationId,
    PrincipalId,
    ApplicationId,
    ResourceId,
    RoleId,
    CapabilityId,
    ScopeId,
    ClientId,
    CredentialId,
    ClientSecretId,
    SessionId,
    EmailVerificationId,
    InvitationId,
    RelyingPartySessionId
);

#[cfg(test)]
#[path = "../tests/unit/identity.rs"]
mod tests;
