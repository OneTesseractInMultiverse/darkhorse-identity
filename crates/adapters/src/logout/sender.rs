use super::{configuration::Settings, network::Policy};
use darkhorse_application::logout::delivery::Sender;
use darkhorse_domain::logout::DeliveryResult;
use std::{
    net::{IpAddr, ToSocketAddrs},
    sync::Arc,
    time::Duration,
};
use tokio::sync::Semaphore;

pub struct HttpsSender {
    policy: Policy,
    certificates: Vec<reqwest::Certificate>,
    dns_slots: Arc<Semaphore>,
}
impl HttpsSender {
    pub fn new(settings: Settings) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        Self {
            policy: settings.policy,
            certificates: settings.certificates,
            dns_slots: Arc::new(Semaphore::new(4)),
        }
    }
    async fn attempt(&self, destination: &str, token: &str) -> DeliveryResult {
        let Ok(target) = super::destination(destination) else {
            return DeliveryResult::Rejected;
        };
        let Ok(ips) = self.resolve(&target).await else {
            return DeliveryResult::Retry;
        };
        let Ok(addresses) = self.policy.addresses(&target, &ips) else {
            return DeliveryResult::Rejected;
        };
        let Ok(client) = client(&target, &addresses, &self.certificates) else {
            return DeliveryResult::Retry;
        };
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("logout_token", token)
            .finish();
        match client
            .post(target)
            .header("content-type", "application/x-www-form-urlencoded")
            .header("cache-control", "no-store")
            .body(body)
            .send()
            .await
        {
            // No response body is consumed or retained. The HTTP/1 parser bounds
            // headers; dropping this nonpooled connection bounds hostile bodies.
            Ok(response) => status(response.status().as_u16()),
            Err(_) => DeliveryResult::Retry,
        }
    }
    async fn resolve(&self, target: &url::Url) -> Result<Vec<IpAddr>, ()> {
        let host = target.host().ok_or(())?;
        match host {
            url::Host::Ipv4(ip) => return Ok(vec![ip.into()]),
            url::Host::Ipv6(ip) => return Ok(vec![ip.into()]),
            url::Host::Domain(_) => (),
        }
        let host = host.to_string();
        let port = target.port_or_known_default().ok_or(())?;
        let permit = self.dns_slots.clone().try_acquire_owned().map_err(|_| ())?;
        tokio::task::spawn_blocking(move || {
            // Cancellation cannot stop an OS resolver; its owned permit remains
            // held until the resolver actually returns, bounding stranded jobs.
            let _permit = permit;
            (host.as_str(), port)
                .to_socket_addrs()
                .map(|addresses| addresses.take(17).map(|a| a.ip()).collect())
                .map_err(|_| ())
        })
        .await
        .map_err(|_| ())?
    }
}
impl Sender for HttpsSender {
    async fn send(&self, destination: &str, token: &str) -> DeliveryResult {
        if token.len() > 8192 {
            return DeliveryResult::Rejected;
        }
        tokio::time::timeout(Duration::from_secs(5), self.attempt(destination, token))
            .await
            .unwrap_or(DeliveryResult::Retry)
    }
}
fn client(
    target: &url::Url,
    addresses: &[std::net::SocketAddr],
    certificates: &[reqwest::Certificate],
) -> Result<reqwest::Client, reqwest::Error> {
    let mut builder = reqwest::Client::builder()
        .https_only(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .http1_only()
        .http1_max_headers(32)
        .pool_max_idle_per_host(0)
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(5))
        .tls_version_min(reqwest::tls::Version::TLS_1_2)
        .tls_sslkeylogfile(false)
        .resolve_to_addrs(target.host_str().unwrap_or_default(), addresses);
    for certificate in certificates {
        builder = builder.add_root_certificate(certificate.clone());
    }
    builder.build()
}
fn status(code: u16) -> DeliveryResult {
    match code {
        200 | 204 => DeliveryResult::Accepted,
        408 | 429 | 500..=599 => DeliveryResult::Retry,
        _ => DeliveryResult::Rejected,
    }
}
#[cfg(test)]
#[path = "../../tests/unit/logout/sender.rs"]
mod tests;
