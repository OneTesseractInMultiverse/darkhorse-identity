use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
};

#[derive(Default)]
pub(super) struct Policy(BTreeMap<String, Vec<IpAddr>>);
impl Policy {
    pub(super) fn parse(text: &str) -> Result<Self, ()> {
        let entries: BTreeMap<String, Vec<IpAddr>> = serde_json::from_str(text).map_err(|_| ())?;
        if entries.len() > 16 {
            return Err(());
        }
        for (origin, ips) in &entries {
            let url = super::destination(&format!("{origin}/")).map_err(|_| ())?;
            if url.origin().ascii_serialization() != *origin
                || ips.is_empty()
                || ips.len() > 16
                || ips.iter().any(|ip| !private(*ip))
            {
                return Err(());
            }
        }
        Ok(Self(entries))
    }
    pub(super) fn addresses(
        &self,
        target: &url::Url,
        ips: &[IpAddr],
    ) -> Result<Vec<SocketAddr>, ()> {
        let approved = self.0.get(&target.origin().ascii_serialization());
        if ips.is_empty()
            || ips.len() > 16
            || ips.iter().any(|ip| match approved {
                Some(approved) => !approved.contains(ip),
                None => !public(*ip),
            })
        {
            return Err(());
        }
        let port = target.port_or_known_default().ok_or(())?;
        Ok(ips.iter().map(|ip| SocketAddr::new(*ip, port)).collect())
    }
}
fn private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_loopback(),
        IpAddr::V6(ip) => ip.is_unique_local() || ip.is_loopback(),
    }
}
// Conservative IANA special-purpose exclusions, including translation and
// transition mechanisms that could route a seemingly public IPv6 address locally.
fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192
                    && (b == 168 || (b == 0 && (c == 0 || c == 2)) || (b == 88 && c == 99)))
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(ip) => {
            let [a, b, ..] = ip.segments();
            (0x2000..=0x3fff).contains(&a)
                && !(a == 0x2001 && (b <= 0x01ff || b == 0x0db8))
                && a != 0x2002
                && !(a == 0x3fff && b <= 0x0fff)
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/logout/network.rs"]
mod tests;
