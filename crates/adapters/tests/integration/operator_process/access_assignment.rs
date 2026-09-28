use super::*;

#[tokio::test]
async fn restricted_runtime_cli_assigns_an_application_role_and_audits_both_revisions() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    fixture.activate().await;
    restrict_database(&fixture).await;
    let (actor_id, email) = actor(&fixture).await;
    let target = Uuid::new_v4();
    let application = Uuid::new_v4();
    let role = Uuid::new_v4();
    sqlx::query("INSERT INTO applications(id,name,owner_id,active) VALUES($1,'CLI assignment fixture',$2,true)")
        .bind(application)
        .bind(actor_id)
        .execute(&fixture.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO roles(id,name) VALUES($1,'CLI assignment fixture role')")
        .bind(role)
        .execute(&fixture.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO role_applications(application_id,role_id) VALUES($1,$2)")
        .bind(application)
        .bind(role)
        .execute(&fixture.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO principals(id,email,first_name,last_name) VALUES($1,$2,'Target','User')",
    )
    .bind(target)
    .bind(format!("target-{target}@example.com"))
    .execute(&fixture.pool)
    .await
    .unwrap();
    let policy_revision: i64 =
        sqlx::query_scalar("SELECT policy_revision FROM security_state WHERE singleton")
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
    let (exit, response) = invoke(
        &["operator", "access", "apply"],
        json!({
            "authentication":{"email":email,"password":PASSWORD,"reason":"Assign approved application role"},
            "policy_revision":policy_revision.to_string(),
            "change":{"operation":"principal_role","principal_id":target.to_string(),"application_id":application.to_string(),"role_id":role.to_string(),"assigned":true,"principal_revision":"0"}
        }),
    );
    assert_eq!(exit, 0, "{response}");
    assert_eq!(response["data"]["completed"], true);
    assert_eq!(response["data"]["changed"], true);
    assert_eq!(response["data"]["principal_revision"], "1");
    assert_eq!(response["data"]["target"]["kind"], "principal_role");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM principal_roles WHERE principal_id=$1 AND application_id=$2 AND role_id=$3")
            .bind(target)
            .bind(application)
            .bind(role)
            .fetch_one(&fixture.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM operator_access_catalog_audit WHERE target_id=$1 AND application_id=$2 AND related_id=$3 AND command='principal.role' AND principal_expected_revision=0 AND principal_resulting_revision=1 AND result='changed' AND database_role='darkhorse_runtime'")
            .bind(target)
            .bind(application)
            .bind(role)
            .fetch_one(&fixture.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM platform_administrators WHERE principal_id=$1",
        )
        .bind(target)
        .fetch_one(&fixture.pool)
        .await
        .unwrap(),
        0
    );
}
