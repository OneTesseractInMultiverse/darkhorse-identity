use super::tokens::setup;
use darkhorse_application::introspection_admission::{Authenticate, Caller, Credentials};
use darkhorse_domain::{identity::ClientId, tokens::Error};
#[tokio::test]
async fn admission_authentication_uses_current_client_secret_and_application_authority() {
    let (db, _) = setup().await;
    let credentials = || Credentials {
        caller: Caller::Client(ClientId::from_u128(0x20).unwrap()),
        secret: [9; 32],
    };
    assert_eq!(
        db.store.authenticate_introspection(credentials()).await,
        Ok(())
    );
    for input in [
        Credentials {
            secret: [8; 32],
            ..credentials()
        },
        Credentials {
            caller: Caller::Client(ClientId::from_u128(0x99).unwrap()),
            ..credentials()
        },
    ] {
        assert_eq!(
            db.store.authenticate_introspection(input).await,
            Err(Error::InvalidClient)
        );
    }
    sqlx::query("UPDATE oauth_client_secrets SET retired=true")
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        db.store.authenticate_introspection(credentials()).await,
        Err(Error::InvalidClient)
    );
    sqlx::query("INSERT INTO oauth_client_secrets(id,client_id,verifier,created_ms) VALUES('00000000-0000-0000-0000-000000000065','00000000-0000-0000-0000-000000000020',$1,0)").bind([7u8;32].as_slice()).execute(&db.pool).await.unwrap();
    let rotated = || Credentials {
        secret: [7; 32],
        ..credentials()
    };
    assert_eq!(db.store.authenticate_introspection(rotated()).await, Ok(()));
    sqlx::query("UPDATE applications SET active=false,revision=revision+1")
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        db.store.authenticate_introspection(rotated()).await,
        Err(Error::InvalidClient)
    );
    db.store.close().await;
    assert_eq!(
        db.store.authenticate_introspection(rotated()).await,
        Err(Error::Unavailable)
    );
}
