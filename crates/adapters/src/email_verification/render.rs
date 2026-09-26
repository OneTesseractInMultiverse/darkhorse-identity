//! Fixed, versioned plaintext templates. Recipient names and labels are not template input.
use darkhorse_application::email_delivery::Delivery;
use darkhorse_domain::{email_verification::Error, localization::Locale};
#[derive(Clone, Copy)]
pub(super) enum Kind {
    Verification,
    Invitation,
}
pub(super) struct Text {
    pub subject: &'static str,
    pub body: String,
}
pub(super) fn render<I>(
    origin: &str,
    token: &str,
    delivery: &Delivery<I>,
    kind: Kind,
) -> Result<Text, Error> {
    validate(origin, token, delivery, kind)?;
    let lifetime = delivery.expires_ms - delivery.created_ms;
    let (path, subject, before, after) = content(kind, delivery.locale, lifetime);
    let locale = crate::localization::tag(delivery.locale);
    let body =
        format!("{before}\r\n\r\n{origin}/{path}#token={token}&lang={locale}\r\n\r\n{after}\r\n");
    if body.len() > 4096 {
        return Err(Error::Invalid);
    }
    Ok(Text { subject, body })
}
fn validate<I>(origin: &str, token: &str, delivery: &Delivery<I>, kind: Kind) -> Result<(), Error> {
    if origin.len() > 1024 || delivery.template_version != 1 {
        return Err(Error::Invalid);
    }
    let url = url::Url::parse(origin).map_err(|_| Error::Invalid)?;
    if url.scheme() != "https" || url.origin().ascii_serialization() != origin {
        return Err(Error::Invalid);
    }
    let expected = match kind {
        Kind::Verification => {
            super::token_digest(token)?;
            darkhorse_domain::email_verification::LIFETIME_MS
        }
        Kind::Invitation => {
            super::invitation_digest(token).map_err(|_| Error::Invalid)?;
            darkhorse_domain::invitations::LIFETIME_MS
        }
    };
    if delivery.expires_ms.checked_sub(delivery.created_ms) != Some(expected) {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn content(
    kind: Kind,
    locale: Locale,
    lifetime: u64,
) -> (&'static str, &'static str, String, &'static str) {
    match (kind, locale) {
        (Kind::Verification, Locale::English) => (
            "security/email",
            "Verify your Darkhorse email",
            format!(
                "Confirm this email for your existing Darkhorse account. Sign in to the same account, then confirm the link below. It expires {} minutes after it was requested and can be used once.",
                lifetime / 60_000
            ),
            "If you did not request this message, you can ignore it. This link cannot reset your password or grant access.",
        ),
        (Kind::Verification, Locale::Spanish) => (
            "security/email",
            "Verifica tu correo de Darkhorse",
            format!(
                "Confirma este correo para tu cuenta existente de Darkhorse. Inicia sesión en la misma cuenta y confirma el enlace de abajo. Vence {} minutos después de la solicitud y solo se puede usar una vez.",
                lifetime / 60_000
            ),
            "Si no solicitaste este mensaje, puedes ignorarlo. Este enlace no permite restablecer tu contraseña ni conceder acceso.",
        ),
        (Kind::Invitation, Locale::English) => (
            "invitation",
            "Your Darkhorse invitation",
            format!(
                "You have been invited to create a Darkhorse account using this email address. Choose your own password using the link below. It expires {} hours after issuance and can be used once. Application access is assigned separately.",
                lifetime / 3_600_000
            ),
            "If you did not expect this invitation, you can ignore it.",
        ),
        (Kind::Invitation, Locale::Spanish) => (
            "invitation",
            "Tu invitación a Darkhorse",
            format!(
                "Has recibido una invitación para crear una cuenta de Darkhorse con este correo. Elige tu propia contraseña mediante el enlace de abajo. Vence {} horas después de su emisión y solo se puede usar una vez. El acceso a las aplicaciones se asigna por separado.",
                lifetime / 3_600_000
            ),
            "Si no esperabas esta invitación, puedes ignorarla.",
        ),
    }
}
#[cfg(test)]
#[path = "../../tests/unit/email_verification/render.rs"]
mod tests;
