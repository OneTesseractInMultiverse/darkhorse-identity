use super::{Secrets, configuration::Settings};
use darkhorse_application::email_verification::{Delivery, DeliveryResult, EmailDelivery};
use darkhorse_domain::email_verification::Error;
use lettre::{
    Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    transport::smtp::{
        authentication::Credentials,
        client::{Certificate, Tls, TlsParameters},
    },
};
use std::{sync::Arc, time::Duration};
#[derive(Clone)]
pub struct Smtp {
    transport: Arc<AsyncSmtpTransport<Tokio1Executor>>,
    secrets: Secrets,
    from: Address,
    origin: String,
}
impl Smtp {
    /// Reads an optional private CA once at startup; hostname verification stays enabled.
    pub fn prepare(settings: Settings, origin: String) -> Result<Self, Error> {
        let ca = settings
            .ca_file
            .as_ref()
            .map(read_ca)
            .transpose()
            .map_err(|_| Error::Invalid)?;
        Self::build(settings, origin, ca.as_deref())
    }
    fn build(settings: Settings, origin: String, ca: Option<&[u8]>) -> Result<Self, Error> {
        let mut tls = TlsParameters::builder(settings.host.clone());
        if let Some(ca) = ca {
            tls = tls.add_root_certificate(certificate(ca)?);
        }
        let mut transport = AsyncSmtpTransport::<Tokio1Executor>::relay(&settings.host)
            .map_err(|_| Error::Invalid)?
            .port(settings.port)
            .tls(Tls::Wrapper(tls.build().map_err(|_| Error::Invalid)?))
            .timeout(Some(Duration::from_secs(5)));
        if !settings.username.is_empty() {
            transport = transport.credentials(Credentials::new(
                settings.username,
                settings.password.to_string(),
            ));
        }
        Ok(Self {
            transport: Arc::new(transport.build()),
            secrets: settings.secrets,
            from: settings.from,
            origin,
        })
    }
}
impl EmailDelivery for Smtp {
    async fn deliver(&self, delivery: &Delivery) -> DeliveryResult {
        let message = match message(&self.from, &self.origin, &self.secrets, delivery) {
            Ok(message) => message,
            Err(_) => return DeliveryResult::Rejected,
        };
        self.send(message).await
    }
}
impl Smtp {
    async fn send(&self, message: Message) -> DeliveryResult {
        match tokio::time::timeout(Duration::from_secs(10), self.transport.send(message)).await {
            Ok(Ok(_)) => DeliveryResult::Accepted,
            Ok(Err(error)) if error.is_permanent() => DeliveryResult::Rejected,
            _ => DeliveryResult::Retry,
        }
    }
}
impl darkhorse_application::invitations::InvitationDelivery for Smtp {
    async fn deliver_invitation(
        &self,
        delivery: &darkhorse_application::invitations::Delivery,
    ) -> DeliveryResult {
        let message = match invitation_message(&self.from, &self.origin, &self.secrets, delivery) {
            Ok(message) => message,
            Err(_) => return DeliveryResult::Rejected,
        };
        self.send(message).await
    }
}
fn legacy_invitation_message(
    from: &Address,
    origin: &str,
    secrets: &Secrets,
    delivery: &darkhorse_application::invitations::Delivery,
) -> Result<Message, Error> {
    let secret = secrets.invitation_token(delivery.seed);
    let token = secret.as_str();
    Message::builder()
        .date(std::time::SystemTime::UNIX_EPOCH+Duration::from_millis(delivery.created_ms))
        .from(from.clone().into()).to(delivery.email.parse::<Address>().map_err(|_|Error::Invalid)?.into())
        .message_id(Some(format!("<invitation-{}@darkhorse.invalid>",uuid::Uuid::from_u128(delivery.id.as_u128()))))
        .subject("Your Darkhorse invitation")
        .body(format!("You have been invited to create a Darkhorse account using this email address. Choose your own password using the link below. It expires 24 hours after issuance and can be used once. Application access is assigned separately.\r\n\r\n{origin}/invitation#token={token}\r\n\r\nIf you did not expect this invitation, you can ignore it.\r\n"))
        .map_err(|_|Error::Invalid)
}

fn legacy_message(
    from: &Address,
    origin: &str,
    secrets: &Secrets,
    delivery: &Delivery,
) -> Result<Message, Error> {
    let secret = secrets.token(delivery.seed);
    let token = secret.as_str();
    Message::builder()
        .date(std::time::SystemTime::UNIX_EPOCH + Duration::from_millis(delivery.created_ms)).from(from.clone().into()).to(delivery.email.parse::<Address>().map_err(|_|Error::Invalid)?.into())
        .message_id(Some(format!("<{}@darkhorse.invalid>",uuid::Uuid::from_u128(delivery.id.as_u128()))))
        .subject("Verify your Darkhorse email")
        .body(format!("Confirm this email for your existing Darkhorse account. Sign in to the same account, then confirm the link below. It expires 15 minutes after it was requested and can be used once.\r\n\r\n{origin}/security/email#token={token}\r\n\r\nIf you did not request this message, you can ignore it. This link cannot reset your password or grant access.\r\n"))
        .map_err(|_|Error::Invalid)
}
fn message(
    from: &Address,
    origin: &str,
    secrets: &Secrets,
    delivery: &Delivery,
) -> Result<Message, Error> {
    if delivery.template_version == 0
        && delivery.locale == darkhorse_domain::localization::Locale::English
    {
        return legacy_message(from, origin, secrets, delivery);
    }
    let token = secrets.token(delivery.seed);
    let text = super::render::render(origin, &token, delivery, super::render::Kind::Verification)?;
    envelope(
        from,
        delivery,
        format!(
            "<{}@darkhorse.invalid>",
            uuid::Uuid::from_u128(delivery.id.as_u128())
        ),
        text,
    )
}
fn invitation_message(
    from: &Address,
    origin: &str,
    secrets: &Secrets,
    delivery: &darkhorse_application::invitations::Delivery,
) -> Result<Message, Error> {
    if delivery.template_version == 0
        && delivery.locale == darkhorse_domain::localization::Locale::English
    {
        return legacy_invitation_message(from, origin, secrets, delivery);
    }
    let token = secrets.invitation_token(delivery.seed);
    let text = super::render::render(origin, &token, delivery, super::render::Kind::Invitation)?;
    envelope(
        from,
        delivery,
        format!(
            "<invitation-{}@darkhorse.invalid>",
            uuid::Uuid::from_u128(delivery.id.as_u128())
        ),
        text,
    )
}
fn envelope<I>(
    from: &Address,
    delivery: &darkhorse_application::email_delivery::Delivery<I>,
    id: String,
    text: super::render::Text,
) -> Result<Message, Error> {
    Message::builder()
        .date(std::time::SystemTime::UNIX_EPOCH + Duration::from_millis(delivery.created_ms))
        .from(from.clone().into())
        .to(delivery
            .email
            .parse::<Address>()
            .map_err(|_| Error::Invalid)?
            .into())
        .message_id(Some(id))
        .subject(text.subject)
        .body(text.body)
        .map_err(|_| Error::Invalid)
}
#[cfg(test)]
#[path = "../../tests/unit/email_verification/smtp.rs"]
mod tests;

fn read_ca(path: &String) -> Result<Vec<u8>, std::io::Error> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(262_145)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}
fn certificate(pem: &[u8]) -> Result<Certificate, Error> {
    use rustls_pki_types::{CertificateDer, pem::PemObject};
    if pem.len() > 262_144 {
        return Err(Error::Invalid);
    }
    let mut certificates = CertificateDer::pem_slice_iter(pem);
    let certificate = certificates
        .next()
        .ok_or(Error::Invalid)?
        .map_err(|_| Error::Invalid)?;
    // This setting adds one trust anchor, not an unchecked certificate bundle.
    if certificates.next().is_some() {
        return Err(Error::Invalid);
    }
    Certificate::from_der(certificate.as_ref().to_vec()).map_err(|_| Error::Invalid)
}
