use super::*;
use sha2::{Digest, Sha256};
use std::time::Duration;

fn settings(endpoint: &str) -> Settings {
    Settings {
        endpoint: endpoint.into(),
        bucket: "media".into(),
        region: "us-east-1".into(),
        access_key: "AKIDEXAMPLE".to_owned().into(),
        secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_owned().into(),
        insecure: false,
    }
}

#[test]
fn signed_requests_preserve_literal_authority_path_payload_and_sensitive_headers() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let id = AssetId::from_u128(1).unwrap();
    for endpoint in [
        "https://media.example.com/",
        "https://s3.amazonaws.com/",
        "https://objects.example.com:9443/",
        "https://[::1]:9443/",
    ] {
        for method in [Method::PUT, Method::GET, Method::DELETE] {
            let body = if method == Method::PUT {
                b"image".as_slice()
            } else {
                &[]
            };
            let request = request(&settings(endpoint), method.clone(), id, body, now).unwrap();
            assert_eq!(request.method(), method);
            assert_eq!(
                request.url().as_str(),
                format!("{endpoint}media/darkhorse/media/00000000000000000000000000000001.png")
            );
            assert!(request.url().query().is_none());
            assert_eq!(request.body().unwrap().as_bytes().unwrap(), body);
            let headers = request.headers();
            assert_eq!(
                headers["x-amz-content-sha256"].to_str().unwrap(),
                Sha256::digest(body)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            );
            assert_eq!(headers["x-amz-date"], "20231114T221320Z");
            let auth = &headers["authorization"];
            assert!(auth.is_sensitive());
            assert_eq!(
                auth.to_str().unwrap(),
                reference_signature(&settings(endpoint), &request, body, now)
            );
            assert!(auth.to_str().unwrap().starts_with(
                "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20231114/us-east-1/s3/aws4_request,"
            ));
            assert!(
                auth.to_str()
                    .unwrap()
                    .contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date")
            );
            assert!(!format!("{request:?}").contains("AKIDEXAMPLE"));
        }
    }
}

// Independent SDK payload hashing must produce the same complete signature as
// the digest-only boundary, for each method, authority and immutable payload.
fn reference_signature(
    settings: &Settings,
    request: &reqwest::Request,
    bytes: &[u8],
    now: SystemTime,
) -> String {
    let identity = Credentials::new(
        settings.access_key.as_str(),
        settings.secret_key.as_str(),
        None,
        None,
        "fixture",
    )
    .into();
    let params = v4::SigningParams::builder()
        .identity(&identity)
        .region(&settings.region)
        .name("s3")
        .time(now)
        .settings(signing_settings())
        .build()
        .unwrap()
        .into();
    let signable = SignableRequest::new(
        request.method().as_str(),
        request.url().as_str(),
        std::iter::empty(),
        SignableBody::Bytes(bytes),
    )
    .unwrap();
    let (instructions, _) = sign(signable, &params).unwrap().into_parts();
    instructions
        .headers()
        .find(|(name, _)| *name == "authorization")
        .unwrap()
        .1
        .to_owned()
}
