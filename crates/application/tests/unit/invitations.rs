use super::*;
use std::sync::Mutex;
struct Fake {
    fail: &'static str,
    calls: Mutex<Vec<&'static str>>,
}
impl Fake {
    fn step(&self, name: &'static str) -> Result<(), Error> {
        self.calls.lock().unwrap().push(name);
        if self.fail == name {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    }
}
fn id() -> InvitationId {
    InvitationId::from_u128(3).unwrap()
}
impl InvitationSecrets for Fake {
    fn issue_invitation(&self) -> Result<Material, Error> {
        self.step("entropy")?;
        Ok(Material {
            id: id(),
            seed: [2; 32],
            digest: [3; 32],
        })
    }
}
impl PasswordPreparation for Fake {
    async fn prepare_password(&self, password: &str) -> Result<PreparedCredential, Error> {
        assert_eq!(password, "a long test password");
        self.step("hash")?;
        Ok(PreparedCredential {
            principal_id: PrincipalId::from_u128(10).unwrap(),
            credential_id: darkhorse_domain::identity::CredentialId::from_u128(11).unwrap(),
            verifier: "prepared".into(),
        })
    }
}
impl InvitationStore for Fake {
    async fn invitation_preflight(&self, _: [u8; 32]) -> Result<(), Error> {
        self.step("preflight")
    }
    async fn invite(
        &self,
        _: [u8; 32],
        email: &str,
        material: Material,
        locale: Option<darkhorse_domain::localization::Locale>,
    ) -> Result<InvitationId, Error> {
        assert_eq!(email, "new@example.com");
        assert_eq!(
            locale,
            Some(darkhorse_domain::localization::Locale::Spanish)
        );
        assert_eq!(material.digest, [3; 32]);
        self.step("invite")?;
        Ok(id())
    }
    async fn invitations(&self, _: [u8; 32]) -> Result<Vec<Record>, Error> {
        unreachable!()
    }
    async fn revoke_invitation(&self, _: [u8; 32], _: InvitationId) -> Result<(), Error> {
        unreachable!()
    }
    async fn admit_invitation(&self, digest: [u8; 32], email: &str) -> Result<InvitationId, Error> {
        assert_eq!(digest, [3; 32]);
        assert_eq!(email, "new@example.com");

        self.step("admit")?;
        Ok(id())
    }
    async fn accept_invitation(
        &self,
        invitation: InvitationId,
        digest: [u8; 32],
        profile: Profile,
        credential: PreparedCredential,
    ) -> Result<PrincipalId, Error> {
        assert_eq!(invitation, id());
        assert_eq!(digest, [3; 32]);
        assert_eq!(profile.first_name(), "New");
        assert_eq!(credential.verifier, "prepared");
        self.step("accept")?;
        Ok(credential.principal_id)
    }
}
impl InvitationQueue for Fake {
    async fn claim_invitation(&self) -> Result<Option<Delivery>, Error> {
        self.step("claim")?;
        Ok((self.fail != "empty").then(|| Delivery {
            id: id(),
            created_ms: 10,
            expires_ms: 86400010,
            locale: darkhorse_domain::localization::Locale::English,
            template_version: 0,
            attempt: 1,
            email: "new@example.com".into(),
            seed: [2; 32],
        }))
    }
    async fn finish_invitation(
        &self,
        invitation: InvitationId,
        attempt: u16,
        result: DeliveryResult,
    ) -> Result<(), Error> {
        assert_eq!(invitation, id());
        assert_eq!(attempt, 1);
        assert_eq!(result, DeliveryResult::Accepted);
        self.step("finish")
    }
}
impl InvitationDelivery for Fake {
    async fn deliver_invitation(&self, _: &Delivery) -> DeliveryResult {
        self.step("send").unwrap();
        DeliveryResult::Accepted
    }
}
fn fake(fail: &'static str) -> Fake {
    Fake {
        fail,
        calls: Mutex::new(vec![]),
    }
}
fn request() -> Acceptance<'static> {
    Acceptance {
        email: "new@example.com",
        first_name: "New",
        last_name: "Person",
        password: "a long test password",
    }
}
#[test]
fn administrator_is_checked_before_entropy_and_all_failures_propagate() {
    run(async {
        for (fail, expected) in [
            ("preflight", vec!["preflight"]),
            ("entropy", vec!["preflight", "entropy"]),
            ("invite", vec!["preflight", "entropy", "invite"]),
            ("", vec!["preflight", "entropy", "invite"]),
        ] {
            let f = fake(fail);
            assert_eq!(
                invite(
                    &f,
                    &f,
                    [1; 32],
                    " new@example.com ",
                    Some(darkhorse_domain::localization::Locale::Spanish)
                )
                .await
                .is_ok(),
                fail.is_empty()
            );
            assert_eq!(*f.calls.lock().unwrap(), expected);
        }
        let f = fake("");
        assert_eq!(
            invite(&f, &f, [1; 32], "bad", None).await,
            Err(Error::Invalid)
        );
        assert!(f.calls.lock().unwrap().is_empty());
    });
}
#[test]
fn invalid_input_and_denied_proofs_never_reach_hashing() {
    run(async {
        for (fail, expected) in [
            ("admit", vec!["admit"]),
            ("hash", vec!["admit", "hash"]),
            ("accept", vec!["admit", "hash", "accept"]),
            ("", vec!["admit", "hash", "accept"]),
        ] {
            let f = fake(fail);
            assert_eq!(
                accept(&f, &f, [3; 32], request()).await.is_ok(),
                fail.is_empty()
            );
            assert_eq!(*f.calls.lock().unwrap(), expected);
        }
        let f = fake("");
        assert_eq!(
            accept(
                &f,
                &f,
                [3; 32],
                Acceptance {
                    password: "short",
                    ..request()
                }
            )
            .await,
            Err(Error::Invalid)
        );
        assert_eq!(
            accept(
                &f,
                &f,
                [3; 32],
                Acceptance {
                    first_name: "",
                    ..request()
                }
            )
            .await,
            Err(Error::Invalid)
        );
        assert!(f.calls.lock().unwrap().is_empty());
    });
}
#[test]
fn delivery_claim_send_finish_are_explicit_and_empty_queues_do_not_send() {
    run(async {
        for (fail, expected, result) in [
            ("claim", vec!["claim"], Err(Error::Unavailable)),
            ("empty", vec!["claim"], Ok(false)),
            (
                "finish",
                vec!["claim", "send", "finish"],
                Err(Error::Unavailable),
            ),
            ("", vec!["claim", "send", "finish"], Ok(true)),
        ] {
            let f = fake(fail);
            assert_eq!(deliver_next(&f, &f).await, result);
            assert_eq!(*f.calls.lock().unwrap(), expected);
        }
    });
}
fn run(future: impl Future<Output = ()>) {
    use std::task::{Context, Poll, Waker};
    assert!(matches!(
        std::pin::pin!(future).poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(())
    ));
}
