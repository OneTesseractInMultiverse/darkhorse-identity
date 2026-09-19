use super::tokens::{ISSUER, input, setup};
use super::*;
use darkhorse_adapters::{
    resource_servers::OsResourceEntropy,
    tokens::material::{self, Purpose},
};
use darkhorse_application::{
    resource_servers::*,
    tokens::{CodeStore, TokenStore},
};
use darkhorse_domain::identity::*;
fn target() -> Target {
    Target {
        application: ApplicationId::from_u128(0x10).unwrap(),
        resource: ResourceId::from_u128(0x30).unwrap(),
    }
}
#[tokio::test]
async fn only_the_resource_credential_sees_current_capabilities_within_the_token_ceiling() {
    let (db, signer) = setup().await;
    super::resource_tokens::policy(&db).await;
    let registry = Service {
        store: db.store.clone(),
        entropy: OsResourceEntropy,
    };
    let registered = registry
        .write(
            [1; 32],
            Command {
                target: target(),
                change: Change::Register,
            },
        )
        .await
        .unwrap();
    let verifier =
        darkhorse_adapters::resource_servers::secret_digest(&registered.secret.unwrap()).unwrap();
    super::resource_tokens::approve(&db, [3; 32]).await;
    let code = db
        .store
        .issue(
            [3; 32],
            Some([1; 32]),
            material::generate(Purpose::Code).unwrap(),
        )
        .await
        .unwrap();
    let token = db
        .store
        .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer)
        .await
        .unwrap();
    let digest = material::digest(&token.access, Purpose::Access).unwrap();
    let probe = || Probe {
        resource: target().resource,
        secret: verifier,
        token: Some(digest),
    };
    let active = db
        .store
        .introspect_resource(probe(), ISSUER)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(active.capabilities.len(), 2);
    sqlx::query(
        "DELETE FROM role_capabilities WHERE capability_id='00000000-0000-0000-0000-000000000071'",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    let narrowed = db
        .store
        .introspect_resource(probe(), ISSUER)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        narrowed.capabilities,
        std::collections::BTreeSet::from([CapabilityId::from_u128(0x70).unwrap()])
    );
    sqlx::query("DELETE FROM principal_roles")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(
        db.store
            .introspect_resource(probe(), ISSUER)
            .await
            .unwrap()
            .is_none()
    );
    db.store.close().await;
}

fn registry(db: &Database) -> Service<PostgresStore, OsResourceEntropy> {
    Service {
        store: db.store.clone(),
        entropy: OsResourceEntropy,
    }
}
async fn fixture() -> (Database, String, [u8; 32]) {
    let (db, signer) = setup().await;
    super::resource_tokens::policy(&db).await;
    let registered = registry(&db)
        .write(
            [1; 32],
            Command {
                target: target(),
                change: Change::Register,
            },
        )
        .await
        .unwrap();
    super::resource_tokens::approve(&db, [3; 32]).await;
    let code = db
        .store
        .issue(
            [3; 32],
            Some([1; 32]),
            material::generate(Purpose::Code).unwrap(),
        )
        .await
        .unwrap();
    let token = db
        .store
        .redeem(input(&code), material::pair().unwrap(), ISSUER, &signer)
        .await
        .unwrap();
    (
        db,
        registered.secret.unwrap(),
        material::digest(&token.access, Purpose::Access).unwrap(),
    )
}
fn probe(secret: &str, token: Option<[u8; 32]>) -> Probe {
    Probe {
        resource: target().resource,
        secret: darkhorse_adapters::resource_servers::secret_digest(secret).unwrap(),
        token,
    }
}
#[tokio::test]
async fn registration_rotation_disabling_and_reenabling_preserve_secret_retirement() {
    use darkhorse_domain::{registration::RegistrationError, tokens::Error};
    let (db, first, token) = fixture().await;
    let registry = registry(&db);
    assert!(matches!(
        registry
            .write(
                [1; 32],
                Command {
                    target: target(),
                    change: Change::Register
                }
            )
            .await,
        Err(RegistrationError::Conflict)
    ));
    let changed = registry
        .write(
            [1; 32],
            Command {
                target: target(),
                change: Change::Rotate {
                    revision: 0,
                    overlap_seconds: 300,
                },
            },
        )
        .await
        .unwrap();
    assert_eq!(changed.record.revision, 1);
    assert_eq!(changed.record.secrets.len(), 2);
    let second = changed.secret.unwrap();
    for secret in [&first, &second] {
        assert!(
            db.store
                .introspect_resource(probe(secret, Some(token)), ISSUER)
                .await
                .unwrap()
                .is_some()
        );
    }
    for (revision, overlap, expected) in [
        (0, 0, RegistrationError::Conflict),
        (1, 301, RegistrationError::Invalid),
    ] {
        assert!(
            matches!(registry.write([1;32],Command {target:target(),change:Change::Rotate {revision,overlap_seconds:overlap}}).await,Err(e) if e==expected)
        );
    }
    let disabled = registry
        .write(
            [1; 32],
            Command {
                target: target(),
                change: Change::SetActive {
                    revision: 1,
                    active: false,
                },
            },
        )
        .await
        .unwrap();
    assert!(!disabled.record.active);
    assert!(disabled.secret.is_none());
    assert!(disabled.record.secrets.is_empty());
    for secret in [&first, &second] {
        assert!(matches!(
            db.store
                .introspect_resource(probe(secret, Some(token)), ISSUER)
                .await,
            Err(Error::InvalidClient)
        ));
    }
    assert!(matches!(
        registry
            .write(
                [1; 32],
                Command {
                    target: target(),
                    change: Change::SetActive {
                        revision: 2,
                        active: true
                    }
                }
            )
            .await,
        Err(RegistrationError::Invalid)
    ));
    let third = registry
        .write(
            [1; 32],
            Command {
                target: target(),
                change: Change::Rotate {
                    revision: 2,
                    overlap_seconds: 0,
                },
            },
        )
        .await
        .unwrap()
        .secret
        .unwrap();
    registry
        .write(
            [1; 32],
            Command {
                target: target(),
                change: Change::SetActive {
                    revision: 3,
                    active: true,
                },
            },
        )
        .await
        .unwrap();
    assert!(
        db.store
            .introspect_resource(probe(&third, Some(token)), ISSUER)
            .await
            .unwrap()
            .is_some()
    );
    for secret in [&first, &second] {
        assert!(matches!(
            db.store
                .introspect_resource(probe(secret, Some(token)), ISSUER)
                .await,
            Err(Error::InvalidClient)
        ));
    }
    assert_eq!(registry.read([1; 32], target()).await.unwrap().revision, 4);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM resource_registration_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        5
    );
    db.store.close().await;
}
#[tokio::test]
async fn every_committed_authority_reduction_is_checked_without_read_audit_writes() {
    use darkhorse_domain::tokens::Error;
    for (statement, authentication_invalid) in [
        ("DELETE FROM principal_roles", false),
        ("DELETE FROM role_capabilities", false),
        (
            "DELETE FROM scope_capabilities; DELETE FROM resource_capabilities",
            false,
        ),
        ("DELETE FROM scope_capabilities", false),
        ("DELETE FROM client_scopes", false),
        (
            "DELETE FROM client_scopes; DELETE FROM client_resources",
            false,
        ),
        ("UPDATE capabilities SET retired=true", false),
        ("UPDATE access_tokens SET revoked=true", false),
        ("UPDATE browser_sessions SET revoked=true", false),
        (
            "UPDATE principals SET credential_epoch=credential_epoch+1,revision=revision+1",
            false,
        ),
        ("DELETE FROM oauth_consents", false),
        (
            "UPDATE oauth_clients SET active=false,revision=revision+1",
            false,
        ),
        (
            "UPDATE applications SET active=false,revision=revision+1",
            true,
        ),
        (
            "UPDATE resource_introspection SET active=false,revision=revision+1",
            true,
        ),
        (
            "UPDATE resource_introspection_secrets SET retired=true",
            true,
        ),
    ] {
        let (db, secret, token) = fixture().await;
        let audit:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM token_audit)+(SELECT count(*) FROM resource_registration_audit)").fetch_one(&db.pool).await.unwrap();
        for _ in 0..3 {
            assert!(
                db.store
                    .introspect_resource(probe(&secret, Some(token)), ISSUER)
                    .await
                    .unwrap()
                    .is_some()
            );
        }
        sqlx::raw_sql(statement).execute(&db.pool).await.unwrap();
        for _ in 0..3 {
            let result = db
                .store
                .introspect_resource(probe(&secret, Some(token)), ISSUER)
                .await;
            if authentication_invalid {
                assert!(matches!(result, Err(Error::InvalidClient)));
            } else {
                assert!(result.unwrap().is_none());
            }
        }
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT (SELECT count(*) FROM token_audit)+(SELECT count(*) FROM resource_registration_audit)").fetch_one(&db.pool).await.unwrap(),audit);
        db.store.close().await;
    }
}
#[tokio::test]
async fn retired_capabilities_shrink_access_and_later_additions_cannot_expand_the_ceiling() {
    let (db, secret, token) = fixture().await;
    sqlx::query(
        "UPDATE capabilities SET retired=true WHERE id='00000000-0000-0000-0000-000000000071'",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    let current = db
        .store
        .introspect_resource(probe(&secret, Some(token)), ISSUER)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        current.capabilities,
        std::collections::BTreeSet::from([CapabilityId::from_u128(0x70).unwrap()])
    );
    sqlx::raw_sql("INSERT INTO capabilities(id,permission_key,meaning) VALUES('00000000-0000-0000-0000-000000000072','extra','new permission'); INSERT INTO capability_applications VALUES('00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000072'); INSERT INTO role_capabilities VALUES('00000000-0000-0000-0000-000000000080','00000000-0000-0000-0000-000000000072'); INSERT INTO resource_capabilities VALUES('00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000030','00000000-0000-0000-0000-000000000072'); INSERT INTO scope_capabilities VALUES('00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000030','00000000-0000-0000-0000-000000000040','00000000-0000-0000-0000-000000000072');").execute(&db.pool).await.unwrap();
    let after = db
        .store
        .introspect_resource(probe(&secret, Some(token)), ISSUER)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.capabilities, current.capabilities);
    db.store.close().await;
}
#[tokio::test]
async fn foreign_audiences_unknown_tokens_and_wrong_credentials_never_disclose_access() {
    use darkhorse_domain::tokens::Error;
    let (db, secret, token) = fixture().await;
    sqlx::raw_sql("INSERT INTO applications(id,name,owner_id,active) VALUES('00000000-0000-0000-0000-000000000011','Other','00000000-0000-0000-0000-000000000001',true); INSERT INTO protected_resources(id,application_id,name,audience) VALUES('00000000-0000-0000-0000-000000000031','00000000-0000-0000-0000-000000000011','Other API','urn:darkhorse:resource:00000000-0000-0000-0000-000000000031');").execute(&db.pool).await.unwrap();
    let other = Target {
        application: ApplicationId::from_u128(0x11).unwrap(),
        resource: ResourceId::from_u128(0x31).unwrap(),
    };
    let other_secret = registry(&db)
        .write(
            [1; 32],
            Command {
                target: other,
                change: Change::Register,
            },
        )
        .await
        .unwrap()
        .secret
        .unwrap();
    let mut foreign = probe(&other_secret, Some(token));
    foreign.resource = other.resource;
    assert!(
        db.store
            .introspect_resource(foreign, ISSUER)
            .await
            .unwrap()
            .is_none()
    );
    for digest in [None, Some([0; 32])] {
        assert!(
            db.store
                .introspect_resource(probe(&secret, digest), ISSUER)
                .await
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            db.store
                .introspect_resource(probe(&other_secret, digest), ISSUER)
                .await,
            Err(Error::InvalidClient)
        ));
    }
    assert!(
        db.store
            .introspect_resource(probe(&secret, Some(token)), "https://other.example")
            .await
            .unwrap()
            .is_none()
    );
    db.store.close().await;
    assert!(matches!(
        db.store
            .introspect_resource(probe(&secret, Some(token)), ISSUER)
            .await,
        Err(Error::Unavailable)
    ));
}
#[tokio::test]
async fn concurrent_permission_changes_and_resource_rotation_are_observed_after_commit() {
    use darkhorse_domain::tokens::Error;
    for secret_change in [false, true] {
        let (db, secret, token) = fixture().await;
        let mut writer = db.pool.begin().await.unwrap();
        let statement = if secret_change {
            "UPDATE resource_introspection_secrets SET retired=true"
        } else {
            "DELETE FROM role_capabilities WHERE capability_id='00000000-0000-0000-0000-000000000071'"
        };
        sqlx::query(statement).execute(&mut *writer).await.unwrap();
        let store = db.store.clone();
        let input = probe(&secret, Some(token));
        let check = tokio::spawn(async move { store.introspect_resource(input, ISSUER).await });
        let mut waiting = false;
        for _ in 0..100 {
            waiting=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT singleton FROM security_state%')").fetch_one(&db.pool).await.unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(waiting);
        writer.commit().await.unwrap();
        let result = check.await.unwrap();
        if secret_change {
            assert!(matches!(result, Err(Error::InvalidClient)));
        } else {
            assert_eq!(result.unwrap().unwrap().capabilities.len(), 1);
        }
        db.store.close().await;
    }
}
#[tokio::test]
async fn failed_registration_audit_rolls_back_and_concurrent_rotation_has_one_winner() {
    use darkhorse_domain::registration::RegistrationError;
    let (db, secret, token) = fixture().await;
    let registry = registry(&db);
    sqlx::raw_sql("CREATE FUNCTION reject_resource_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$; CREATE TRIGGER reject_resource_audit BEFORE INSERT ON resource_registration_audit FOR EACH ROW EXECUTE FUNCTION reject_resource_audit();").execute(&db.pool).await.unwrap();
    let rotate = Command {
        target: target(),
        change: Change::Rotate {
            revision: 0,
            overlap_seconds: 0,
        },
    };
    assert!(matches!(
        registry.write([1; 32], rotate).await,
        Err(RegistrationError::Unavailable)
    ));
    assert!(
        db.store
            .introspect_resource(probe(&secret, Some(token)), ISSUER)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(registry.read([1; 32], target()).await.unwrap().revision, 0);
    sqlx::query("DROP TRIGGER reject_resource_audit ON resource_registration_audit")
        .execute(&db.pool)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        registry.write([1; 32], rotate),
        registry.write([1; 32], rotate)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let (good, bad) = if a.is_ok() { (a, b) } else { (b, a) };
    assert!(matches!(bad, Err(RegistrationError::Conflict)));
    let current = good.unwrap().secret.unwrap();
    assert!(
        db.store
            .introspect_resource(probe(&current, Some(token)), ISSUER)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        db.store
            .introspect_resource(probe(&secret, Some(token)), ISSUER)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM resource_registration_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        2
    );
    db.store.close().await;
}

#[tokio::test]
async fn resource_registration_rechecks_live_administrator_session_and_target() {
    use darkhorse_domain::registration::RegistrationError as E;
    let (db, _) = setup().await;
    super::resource_tokens::policy(&db).await;
    registry(&db)
        .write(
            [1; 32],
            Command {
                target: target(),
                change: Change::Register,
            },
        )
        .await
        .unwrap();
    let command = Command {
        target: target(),
        change: Change::Rotate {
            revision: 0,
            overlap_seconds: 0,
        },
    };
    assert!(matches!(
        registry(&db).write([0; 32], command).await,
        Err(E::Unauthorized)
    ));
    let other = Target {
        application: ApplicationId::from_u128(0x11).unwrap(),
        ..target()
    };
    assert!(matches!(
        registry(&db)
            .write(
                [1; 32],
                Command {
                    target: other,
                    ..command
                }
            )
            .await,
        Err(E::NotFound)
    ));
    assert!(matches!(
        registry(&db).read([1; 32], other).await,
        Err(E::NotFound)
    ));
    let absent = Target {
        resource: ResourceId::from_u128(0x99).unwrap(),
        ..target()
    };
    assert!(matches!(
        registry(&db)
            .write(
                [1; 32],
                Command {
                    target: absent,
                    ..command
                }
            )
            .await,
        Err(E::NotFound)
    ));
    Store::preflight(&db.store, [1; 32], command).await.unwrap();
    super::insert_principal(&db, 2, true).await;
    sqlx::query("DELETE FROM platform_administrators WHERE principal_id='00000000-0000-0000-0000-000000000001'").execute(&db.pool).await.unwrap();
    let verifier = OsResourceEntropy.secret().unwrap().verifier;
    assert!(matches!(
        Store::execute(&db.store, [1; 32], command, Some(verifier)).await,
        Err(E::Forbidden)
    ));
    assert!(matches!(
        registry(&db).read([1; 32], target()).await,
        Err(E::Forbidden)
    ));
    sqlx::query(
        "INSERT INTO platform_administrators VALUES('00000000-0000-0000-0000-000000000001')",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE browser_sessions SET created_ms=created_ms-300000,expires_ms=expires_ms-300000 WHERE digest=$1").bind([1u8;32].as_slice()).execute(&db.pool).await.unwrap();
    assert!(matches!(
        registry(&db).write([1; 32], command).await,
        Err(E::RecentAuthenticationRequired)
    ));
    assert_eq!(
        registry(&db)
            .read([1; 32], target())
            .await
            .unwrap()
            .revision,
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM resource_registration_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    db.store.close().await;
}
#[tokio::test]
async fn resource_database_guards_prevent_rebinding_resurrection_and_audit_mutation() {
    let (db, _, _) = fixture().await;
    for statement in [
        "UPDATE resource_introspection SET application_id='00000000-0000-0000-0000-000000000011',revision=revision+1",
        "UPDATE resource_introspection SET resource_id='00000000-0000-0000-0000-000000000031',revision=revision+1",
        "UPDATE resource_introspection SET active=false",
        "DELETE FROM resource_introspection",
        "UPDATE resource_introspection_secrets SET verifier=decode(repeat('ee',32),'hex')",
        "UPDATE resource_introspection_secrets SET created_ms=created_ms+1",
        "UPDATE resource_introspection_secrets SET expires_ms=floor(extract(epoch FROM clock_timestamp())*1000)::bigint+400000",
        "DELETE FROM resource_introspection_secrets",
        "UPDATE resource_registration_audit SET revision=revision+1",
        "DELETE FROM resource_registration_audit",
        "UPDATE access_tokens SET credential_id=gen_random_uuid()",
    ] {
        assert!(
            sqlx::query(statement).execute(&db.pool).await.is_err(),
            "{statement}"
        );
    }
    registry(&db)
        .write(
            [1; 32],
            Command {
                target: target(),
                change: Change::Rotate {
                    revision: 0,
                    overlap_seconds: 0,
                },
            },
        )
        .await
        .unwrap();
    for statement in [
        "UPDATE resource_introspection_secrets SET expires_ms=NULL WHERE expires_ms IS NOT NULL",
        "UPDATE resource_introspection_secrets SET expires_ms=expires_ms+1 WHERE expires_ms IS NOT NULL",
    ] {
        assert!(sqlx::query(statement).execute(&db.pool).await.is_err());
    }
    sqlx::query("UPDATE resource_introspection_secrets SET retired=true")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(
        sqlx::query("UPDATE resource_introspection_secrets SET retired=false")
            .execute(&db.pool)
            .await
            .is_err()
    );
    db.store.close().await;
}
#[tokio::test]
async fn missing_authority_and_oversize_graph_fail_unavailable_without_truncated_access() {
    for statement in [
        "ALTER TABLE resource_capabilities RENAME COLUMN capability_id TO unavailable",
        "INSERT INTO capabilities(id,permission_key,meaning) SELECT lpad(to_hex(i),32,'0')::uuid,'extra-'||i,'test' FROM generate_series(1000,1254) AS i; INSERT INTO capability_applications SELECT '00000000-0000-0000-0000-000000000010',id FROM capabilities WHERE permission_key LIKE 'extra-%'; INSERT INTO resource_capabilities SELECT '00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000030',id FROM capabilities WHERE permission_key LIKE 'extra-%';",
        "INSERT INTO roles(id,name) SELECT lpad(to_hex(i),32,'0')::uuid,'extra' FROM generate_series(1000,1063) AS i; INSERT INTO role_applications SELECT '00000000-0000-0000-0000-000000000010',id FROM roles WHERE name='extra'; INSERT INTO principal_roles SELECT '00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000010',id FROM roles WHERE name='extra';",
    ] {
        let (db, secret, token) = fixture().await;
        sqlx::raw_sql(statement).execute(&db.pool).await.unwrap();
        assert!(matches!(
            db.store
                .introspect_resource(probe(&secret, Some(token)), ISSUER)
                .await,
            Err(darkhorse_domain::tokens::Error::Unavailable)
        ));
        db.store.close().await;
    }
}
#[tokio::test]
async fn a_resource_secret_expiring_while_a_token_read_waits_is_rejected() {
    let (db, secret, token) = fixture().await;
    sqlx::query("UPDATE resource_introspection_secrets SET expires_ms=floor(extract(epoch FROM clock_timestamp())*1000)::bigint+1500").execute(&db.pool).await.unwrap();
    let mut blocker = db.pool.begin().await.unwrap();
    sqlx::query("SELECT digest FROM authorization_codes FOR UPDATE")
        .fetch_all(&mut *blocker)
        .await
        .unwrap();
    let store = db.store.clone();
    let input = probe(&secret, Some(token));
    let check = tokio::spawn(async move { store.introspect_resource(input, ISSUER).await });
    let mut waiting = false;
    for _ in 0..100 {
        waiting=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT * FROM authorization_codes%')").fetch_one(&db.pool).await.unwrap();
        if waiting {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(waiting);
    sqlx::query("SELECT pg_sleep(GREATEST(0,(expires_ms-floor(extract(epoch FROM clock_timestamp())*1000)::bigint)::double precision/1000)+0.02) FROM resource_introspection_secrets").execute(&db.pool).await.unwrap();
    blocker.rollback().await.unwrap();
    assert!(matches!(
        check.await.unwrap(),
        Err(darkhorse_domain::tokens::Error::InvalidClient)
    ));
    db.store.close().await;
}

#[tokio::test]
async fn expired_and_not_yet_valid_resource_tokens_never_authorize() {
    for offset in [-300001_i64, 300001] {
        let (db, secret, token) = fixture().await;
        // Move only the disposable token's clock fixture, then restore the immutable guard.
        sqlx::query("ALTER TABLE access_tokens DISABLE TRIGGER access_transition")
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE access_tokens SET created_ms=created_ms+$1,expires_ms=expires_ms+$1")
            .bind(offset)
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("ALTER TABLE access_tokens ENABLE TRIGGER access_transition")
            .execute(&db.pool)
            .await
            .unwrap();
        assert!(
            db.store
                .introspect_resource(probe(&secret, Some(token)), ISSUER)
                .await
                .unwrap()
                .is_none()
        );
        db.store.close().await;
    }
}

#[tokio::test]
async fn bounded_projection_decodes_sparse_roles_at_capacity_without_expanding_the_ceiling() {
    let (db, secret, token) = fixture().await;
    sqlx::raw_sql("INSERT INTO capabilities(id,permission_key,meaning) SELECT lpad(to_hex(i),32,'0')::uuid,'bounded-'||i,'test' FROM generate_series(1000,1253) AS i;
        INSERT INTO capability_applications SELECT '00000000-0000-0000-0000-000000000010',id FROM capabilities WHERE permission_key LIKE 'bounded-%';
        INSERT INTO resource_capabilities SELECT '00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000030',id FROM capabilities WHERE permission_key LIKE 'bounded-%';
        INSERT INTO roles(id,name) SELECT lpad(to_hex(i),32,'0')::uuid,'sparse' FROM generate_series(2000,2062) AS i;
        INSERT INTO role_applications SELECT '00000000-0000-0000-0000-000000000010',id FROM roles WHERE name='sparse';
        INSERT INTO principal_roles SELECT '00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000010',id FROM roles WHERE name='sparse';
        INSERT INTO role_capabilities SELECT '00000000-0000-0000-0000-000000000080',id FROM capabilities WHERE permission_key LIKE 'bounded-%';")
        .execute(&db.pool).await.unwrap();
    // 256 live capabilities and 64 roles, including empty and unequal bindings.
    let result = db
        .store
        .introspect_resource(probe(&secret, Some(token)), ISSUER)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        result.capabilities,
        std::collections::BTreeSet::from([
            CapabilityId::from_u128(0x70).unwrap(),
            CapabilityId::from_u128(0x71).unwrap()
        ])
    );
    sqlx::query(
        "UPDATE capabilities SET retired=true WHERE id='00000000-0000-0000-0000-000000000071'",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    let result = db
        .store
        .introspect_resource(probe(&secret, Some(token)), ISSUER)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        result.capabilities,
        std::collections::BTreeSet::from([CapabilityId::from_u128(0x70).unwrap()])
    );
    sqlx::query("DELETE FROM principal_roles")
        .execute(&db.pool)
        .await
        .unwrap();
    assert!(
        db.store
            .introspect_resource(probe(&secret, Some(token)), ISSUER)
            .await
            .unwrap()
            .is_none()
    );
    db.store.close().await;
}
