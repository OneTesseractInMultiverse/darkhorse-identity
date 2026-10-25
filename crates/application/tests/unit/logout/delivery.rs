use super::*;
use crate::{
    logout::LogoutClaims,
    signing::{PublicKey, WrappedKey},
};
use darkhorse_domain::identity::*;
use std::{
    sync::Mutex,
    task::{Context, Poll, Waker},
};
struct Store {
    available: bool,
    failure: bool,
    finish_failure: bool,
    result: Mutex<Option<DeliveryResult>>,
}
impl Queue for Store {
    async fn claim(&self) -> Result<Option<Delivery>, Error> {
        if self.failure {
            return Err(Error::Unavailable);
        }
        Ok(self.available.then(|| Delivery {
            id: LogoutDeliveryId::from_u128(1).unwrap(),
            attempt: 1,
            destination: "https://client.example/logout".into(),
            key: WrappedKey {
                public: PublicKey {
                    kid: "key".into(),
                    n: "n".into(),
                    e: "e".into(),
                },
                nonce: [0; 12],
                ciphertext: vec![],
            },
            claims: LogoutClaims {
                issuer: "https://issuer.example".into(),
                client: ClientId::from_u128(1).unwrap(),
                subject: PrincipalId::from_u128(2).unwrap(),
                session: RelyingPartySessionId::from_u128(3).unwrap(),
                token: LogoutTokenId::from_u128(4).unwrap(),
                issued: 1,
            },
        }))
    }
    async fn finish(
        &self,
        id: LogoutDeliveryId,
        attempt: u16,
        result: DeliveryResult,
    ) -> Result<(), Error> {
        assert_eq!((id.as_u128(), attempt), (1, 1));
        if self.finish_failure {
            return Err(Error::Unavailable);
        }
        *self.result.lock().unwrap() = Some(result);
        Ok(())
    }
}
struct Signer(bool);
impl LogoutSigner for Signer {
    async fn sign_logout(&self, _: WrappedKey, _: LogoutClaims) -> Result<String, Error> {
        if self.0 {
            Ok("signed".into())
        } else {
            Err(Error::Unavailable)
        }
    }
}
struct Transport {
    calls: Mutex<usize>,
    result: DeliveryResult,
}
impl Sender for Transport {
    async fn send(&self, destination: &str, token: &str) -> DeliveryResult {
        assert_eq!(destination, "https://client.example/logout");
        assert_eq!(token, "signed");
        *self.calls.lock().unwrap() += 1;
        self.result
    }
}
#[test]
fn delivery_never_sends_without_a_durable_claim_and_valid_signature() {
    let future = async {
        for (available, failure, signs, finish_failure, expected, calls) in [
            (false, false, true, false, Ok(false), 0),
            (true, true, true, false, Err(Error::Unavailable), 0),
            (true, false, false, false, Ok(true), 0),
            (true, false, true, false, Ok(true), 1),
            (true, false, true, true, Err(Error::Unavailable), 1),
        ] {
            let store = Store {
                available,
                failure,
                finish_failure,
                result: Mutex::new(None),
            };
            let transport = Transport {
                calls: Mutex::new(0),
                result: DeliveryResult::Accepted,
            };
            assert_eq!(
                deliver_next(&store, &Signer(signs), &transport).await,
                expected
            );
            assert_eq!(*transport.calls.lock().unwrap(), calls);
            if available && !failure && !finish_failure {
                assert_eq!(
                    *store.result.lock().unwrap(),
                    Some(if signs {
                        DeliveryResult::Accepted
                    } else {
                        DeliveryResult::Retry
                    })
                );
            }
        }
    };
    assert!(matches!(
        std::pin::pin!(future).poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(())
    ));
}
