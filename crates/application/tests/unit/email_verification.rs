use super::*;
use std::sync::Mutex;
struct Queue {
    available: bool,
    outcome: Mutex<Option<DeliveryResult>>,
    fail: bool,
}
impl DeliveryQueue for Queue {
    async fn claim_email(&self) -> Result<Option<Delivery>, Error> {
        if self.fail {
            return Err(Error::Unavailable);
        }
        Ok(self.available.then(|| Delivery {
            created_ms: 100,
            expires_ms: 900100,
            locale: darkhorse_domain::localization::Locale::English,
            template_version: 0,
            id: EmailVerificationId::from_u128(1).unwrap(),
            attempt: 1,
            email: "a@example.com".into(),
            seed: [1; 32],
        }))
    }
    async fn finish_email(
        &self,
        id: EmailVerificationId,
        attempt: u16,
        result: DeliveryResult,
    ) -> Result<(), Error> {
        assert_eq!(id.as_u128(), 1);
        assert_eq!(attempt, 1);
        *self.outcome.lock().unwrap() = Some(result);
        Ok(())
    }
}
struct Sender(DeliveryResult);
impl EmailDelivery for Sender {
    async fn deliver(&self, _: &Delivery) -> DeliveryResult {
        self.0
    }
}
#[test]
fn empty_failed_and_claimed_queues_have_explicit_outcomes() {
    run(async {
        for outcome in [
            DeliveryResult::Accepted,
            DeliveryResult::Retry,
            DeliveryResult::Rejected,
        ] {
            let q = Queue {
                available: true,
                outcome: Mutex::new(None),
                fail: false,
            };
            assert_eq!(deliver_next(&q, &Sender(outcome)).await, Ok(true));
            assert_eq!(*q.outcome.lock().unwrap(), Some(outcome));
        }
        let q = Queue {
            available: false,
            outcome: Mutex::new(None),
            fail: false,
        };
        assert_eq!(
            deliver_next(&q, &Sender(DeliveryResult::Accepted)).await,
            Ok(false)
        );
        let q = Queue { fail: true, ..q };
        assert_eq!(
            deliver_next(&q, &Sender(DeliveryResult::Accepted)).await,
            Err(Error::Unavailable)
        );
        assert_eq!(*q.outcome.lock().unwrap(), None);
    });
}
struct Store(bool);
impl VerificationStore for Store {
    async fn email_status(&self, _: [u8; 32]) -> Result<Status, Error> {
        if !self.0 {
            return Err(Error::Unauthorized);
        }
        Ok(Status {
            email: "a@example.com".into(),
            verified: false,
        })
    }
    async fn request_verification(&self, actor: [u8; 32], material: Material) -> Result<(), Error> {
        assert_eq!(actor, [1; 32]);
        assert_eq!(material.seed, [2; 32]);
        Ok(())
    }
    async fn verify_email(&self, _: [u8; 32], _: [u8; 32]) -> Result<(), Error> {
        unreachable!()
    }
}
struct Secrets(bool);
impl VerificationSecrets for Secrets {
    fn issue(&self) -> Result<Material, Error> {
        if !self.0 {
            return Err(Error::Unavailable);
        }
        Ok(Material {
            id: EmailVerificationId::from_u128(1).unwrap(),
            seed: [2; 32],
            digest: [3; 32],
        })
    }
}
#[test]
fn request_checks_actor_before_entropy_and_propagates_failure() {
    run(async {
        assert_eq!(
            request(&Store(false), &Secrets(false), [1; 32]).await,
            Err(Error::Unauthorized)
        );
        assert_eq!(
            request(&Store(true), &Secrets(false), [1; 32]).await,
            Err(Error::Unavailable)
        );
        assert_eq!(request(&Store(true), &Secrets(true), [1; 32]).await, Ok(()));
    });
}

fn run(future: impl Future<Output = ()>) {
    use std::task::{Context, Poll, Waker};
    assert!(matches!(
        std::pin::pin!(future).poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(())
    ));
}
