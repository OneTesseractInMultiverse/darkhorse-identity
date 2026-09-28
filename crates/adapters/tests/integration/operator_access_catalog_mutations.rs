use super::{
    Database, insert_password, insert_principal, oidc,
    operator_directory::{Allow, Password},
};
use darkhorse_application::{
    admin_catalog::Written,
    operator_accounts,
    registration::{Entropy, NewSecret},
};
use darkhorse_domain::{
    admin_catalog::Change,
    identity::{ApplicationId, OperationId},
    operator_accounts::Error,
    operator_catalog::MutationRequest,
    registration::{Label, RegistrationError},
};
use std::{
    num::NonZeroU128,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use uuid::Uuid;

#[derive(Clone, Default)]
struct Material(Arc<AtomicUsize>);
impl Entropy for Material {
    fn identifier(&self) -> Result<NonZeroU128, RegistrationError> {
        let next = 10_000 + self.0.fetch_add(1, Ordering::SeqCst) as u128;
        Ok(NonZeroU128::new(next).unwrap())
    }
    fn secret(&self) -> Result<NewSecret, RegistrationError> {
        panic!("access-catalog writes do not issue client secrets")
    }
}

async fn revision(db: &Database) -> u64 {
    sqlx::query_scalar::<_, i64>("SELECT policy_revision FROM security_state")
        .fetch_one(&db.pool)
        .await
        .unwrap()
        .try_into()
        .unwrap()
}

async fn write(
    db: &Database,
    email: &str,
    expected_revision: u64,
    change: Change,
) -> Result<Written, Error> {
    write_with_material(db, email, expected_revision, change, Material::default()).await
}

async fn write_with_material(
    db: &Database,
    email: &str,
    expected_revision: u64,
    change: Change,
    material: Material,
) -> Result<Written, Error> {
    operator_accounts::run(
        &db.store.operator_access_catalog(material),
        &Allow,
        &Password(true),
        OperationId::from_u128(Uuid::new_v4().as_u128()).unwrap(),
        MutationRequest::new(expected_revision, change, "Approved policy change").unwrap(),
        email,
        "source-only passphrase",
    )
    .await
}

fn create_role(application: ApplicationId) -> Change {
    Change::CreateRole {
        name: Label::new("CLI reader role").unwrap(),
        application: Some(application),
    }
}

#[tokio::test]
async fn every_shared_access_catalog_change_uses_the_same_policy_writer() {
    let db = oidc::fixture().await;
    let app = ApplicationId::from_u128(16).unwrap();
    sqlx::query("INSERT INTO protected_resources(id,application_id,name,audience) VALUES($1,$2,'CLI resource',$3)")
        .bind(Uuid::from_u128(800))
        .bind(Uuid::from_u128(app.as_u128()))
        .bind(format!("urn:darkhorse:resource:{}", Uuid::from_u128(800)))
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO resource_scopes(id,application_id,resource_id,name) VALUES($1,$2,$3,'cli:read')")
        .bind(Uuid::from_u128(801))
        .bind(Uuid::from_u128(app.as_u128()))
        .bind(Uuid::from_u128(800))
        .execute(&db.pool)
        .await
        .unwrap();
    let capability = write(
        &db,
        "one@example.com",
        revision(&db).await,
        Change::CreateCapability {
            definition: darkhorse_domain::admin_catalog::PermissionDefinition::new(
                "reports.read",
                "Read reporting data",
            )
            .unwrap(),
            application: None,
        },
    )
    .await
    .unwrap();
    let darkhorse_application::admin_catalog::Target::Capability(capability) = capability.target
    else {
        panic!("created capability target")
    };
    let role = write(
        &db,
        "one@example.com",
        revision(&db).await,
        Change::CreateRole {
            name: Label::new("CLI reader role").unwrap(),
            application: None,
        },
    )
    .await
    .unwrap();
    let darkhorse_application::admin_catalog::Target::Role(role) = role.target else {
        panic!("created role target")
    };

    let changes = [
        Change::CapabilityBinding {
            application: app,
            capability,
            bound: true,
        },
        Change::RoleBinding {
            application: app,
            role,
            bound: true,
        },
        Change::RoleCapability {
            role,
            capability,
            granted: true,
        },
        Change::ResourceCapability {
            application: app,
            resource: darkhorse_domain::identity::ResourceId::from_u128(800).unwrap(),
            capability,
            exposed: true,
        },
        Change::ScopeCapability {
            application: app,
            resource: darkhorse_domain::identity::ResourceId::from_u128(800).unwrap(),
            scope: darkhorse_domain::identity::ScopeId::from_u128(801).unwrap(),
            capability,
            included: true,
        },
        Change::ScopeCapability {
            application: app,
            resource: darkhorse_domain::identity::ResourceId::from_u128(800).unwrap(),
            scope: darkhorse_domain::identity::ScopeId::from_u128(801).unwrap(),
            capability,
            included: false,
        },
        Change::ResourceCapability {
            application: app,
            resource: darkhorse_domain::identity::ResourceId::from_u128(800).unwrap(),
            capability,
            exposed: false,
        },
        Change::RoleCapability {
            role,
            capability,
            granted: false,
        },
        Change::RoleBinding {
            application: app,
            role,
            bound: false,
        },
        Change::CapabilityBinding {
            application: app,
            capability,
            bound: false,
        },
        Change::RetireCapability(capability),
        Change::RetireCapability(capability),
    ];
    let mut previous_revision = revision(&db).await;
    for change in changes {
        let written = write(&db, "one@example.com", previous_revision, change)
            .await
            .unwrap();
        assert!(written.policy_revision >= previous_revision);
        previous_revision = written.policy_revision;
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM operator_access_catalog_audit WHERE result='changed'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        13
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM operator_access_catalog_audit WHERE result='unchanged'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM role_capabilities WHERE role_id=$1 OR capability_id=$2"
        )
        .bind(Uuid::from_u128(role.as_u128()))
        .bind(Uuid::from_u128(capability.as_u128()))
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        0
    );
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT retired FROM capabilities WHERE id=$1")
            .bind(Uuid::from_u128(capability.as_u128()))
            .fetch_one(&db.pool)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn create_role_shares_policy_validation_and_commits_only_operator_audit() {
    let db = oidc::fixture().await;
    let before = revision(&db).await;
    let written = write(
        &db,
        "one@example.com",
        before,
        create_role(ApplicationId::from_u128(16).unwrap()),
    )
    .await
    .unwrap();
    assert!(written.policy_revision > before);
    let role = Uuid::from_u128(match written.target {
        darkhorse_application::admin_catalog::Target::Role(id) => id.as_u128(),
        _ => panic!("created role target"),
    });
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM roles WHERE id=$1")
            .bind(role)
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM role_applications WHERE role_id=$1 AND application_id=$2"
        )
        .bind(role)
        .bind(Uuid::from_u128(16))
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
    let audit: (String, i64, i64, String) = sqlx::query_as(
        "SELECT command,expected_revision,resulting_revision,result FROM operator_access_catalog_audit WHERE target_id=$1",
    )
    .bind(role)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        audit,
        (
            "role.create".into(),
            before as i64,
            written.policy_revision as i64,
            "changed".into()
        )
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM catalog_admin_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    let captured: String = sqlx::query_scalar(
        "SELECT row_to_json(a)::text FROM operator_access_catalog_audit a WHERE target_id=$1",
    )
    .bind(role)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(!captured.contains("CLI reader role"));
    assert!(!captured.contains("one@example.com"));
    assert!(!captured.contains("source-only passphrase"));
}

#[tokio::test]
async fn stale_revision_and_nonadministrator_requests_have_audit_but_no_policy_effect() {
    let db = oidc::fixture().await;
    let stale = revision(&db).await;
    sqlx::query("INSERT INTO capabilities(id,permission_key,meaning) VALUES($1,'revision.bump','Revision bump')")
        .bind(Uuid::from_u128(700))
        .execute(&db.pool)
        .await
        .unwrap();
    let before_attempt = revision(&db).await;
    let material = Material::default();
    assert!(matches!(
        write_with_material(
            &db,
            "one@example.com",
            stale,
            create_role(ApplicationId::from_u128(16).unwrap()),
            material.clone()
        )
        .await,
        Err(Error::Conflict)
    ));
    assert_eq!(material.0.load(Ordering::SeqCst), 0);
    assert_eq!(revision(&db).await, before_attempt);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM roles WHERE name='CLI reader role'")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );

    insert_principal(&db, 2, false).await;
    insert_password(&db, 2).await;
    let nonadministrator_revision = revision(&db).await;
    assert!(matches!(
        write(
            &db,
            "person2@example.com",
            nonadministrator_revision,
            create_role(ApplicationId::from_u128(16).unwrap())
        )
        .await,
        Err(Error::Denied)
    ));
    assert_eq!(revision(&db).await, nonadministrator_revision);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM roles WHERE name='CLI reader role'")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM operator_access_catalog_audit WHERE result='conflict'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operator_access_catalog_audit WHERE result='denied' AND actor_id=$1").bind(Uuid::from_u128(2)).fetch_one(&db.pool).await.unwrap(), 1);
}

#[tokio::test]
async fn required_operator_audit_failure_rolls_back_catalog_and_revision_changes() {
    let db = oidc::fixture().await;
    sqlx::raw_sql("CREATE FUNCTION reject_access_write_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture audit rejection'; END $$; CREATE TRIGGER reject_access_write_audit BEFORE INSERT ON operator_access_catalog_audit FOR EACH ROW EXECUTE FUNCTION reject_access_write_audit();")
        .execute(&db.pool)
        .await
        .unwrap();
    let before = revision(&db).await;
    assert!(matches!(
        write(
            &db,
            "one@example.com",
            before,
            create_role(ApplicationId::from_u128(16).unwrap())
        )
        .await,
        Err(Error::Unavailable)
    ));
    assert_eq!(revision(&db).await, before);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM roles WHERE name='CLI reader role'")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operator_access_catalog_audit")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
}
