use super::{Fixture, SERIAL, variable};
use darkhorse_adapters::{login_admission::SharedLoginAdmission, password::PasswordPreparation};
use darkhorse_application::{
    authentication::{AuthError, LoginAdmission},
    bootstrap::CredentialPreparation,
};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};
use uuid::Uuid;
const PASSWORD: &str = "source-only operator passphrase";
async fn restrict_database(f: &Fixture) {
    sqlx::raw_sql("DO $$ BEGIN IF NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname='darkhorse_runtime') THEN CREATE ROLE darkhorse_runtime LOGIN PASSWORD 'source-only-runtime-fixture'; CREATE ROLE darkhorse_owner NOLOGIN; CREATE ROLE darkhorse_operator LOGIN PASSWORD 'source-only-operator-fixture'; END IF; END $$;").execute(&f.pool).await.unwrap();
    let grants = include_str!("../../../../deploy/grant-runtime.sql")
        .lines()
        .filter(|line| !line.starts_with('\\'))
        .collect::<Vec<_>>()
        .join("\n");
    // Embedded repository SQL only; no caller input is interpolated.
    sqlx::raw_sql(sqlx::AssertSqlSafe(grants))
        .execute(&f.pool)
        .await
        .unwrap();
}
async fn actor(f: &Fixture) -> (Uuid, String) {
    let prepared = PasswordPreparation::default()
        .prepare(PASSWORD)
        .await
        .unwrap();
    let id = Uuid::from_u128(prepared.principal_id.as_u128());
    let credential = Uuid::from_u128(prepared.credential_id.as_u128());
    let email = format!("operator-{id}@example.com");
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO principals(id,email,first_name,last_name) VALUES($1,$2,'Operator','Fixture')",
    )
    .bind(id)
    .bind(&email)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO credentials(id,principal_id,kind) VALUES($1,$2,'password')")
        .bind(credential)
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO password_credentials(credential_id,verifier) VALUES($1,$2)")
        .bind(credential)
        .bind(prepared.verifier)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO platform_administrators(principal_id) VALUES($1)")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (id, email)
}
fn invoke(args: &[&str], input: Value) -> (i32, Value) {
    invoke_as("runtime", args, input)
}
fn invoke_as(role: &str, args: &[&str], input: Value) -> (i32, Value) {
    invoke_output(role, args, input, false)
}
fn invoke_output(role: &str, args: &[&str], input: Value, closed_stdout: bool) -> (i32, Value) {
    let mut command = Command::new(variable("DARKHORSE_TEST_SERVER_PATH"));
    command.env_clear().env("PATH", variable("PATH"));
    for name in [
        "DARKHORSE_DATABASE_URL",
        "DARKHORSE_DATABASE_INSECURE",
        "DARKHORSE_REDIS_CACHE_URL",
        "DARKHORSE_REDIS_LIMITER_URL",
        "DARKHORSE_REDIS_INSECURE",
    ] {
        command.env(name, variable(name));
    }
    if let Ok(value) = std::env::var("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", value);
    }
    let mut database = url::Url::parse(&variable("DARKHORSE_DATABASE_URL")).unwrap();
    database.set_username(&format!("darkhorse_{role}")).unwrap();
    database
        .set_password(Some(&format!("source-only-{role}-fixture")))
        .unwrap();
    command.env("DARKHORSE_DATABASE_URL", database.as_str());
    command
        .env("DARKHORSE_LOGIN_ENABLED", "true")
        .env("DARKHORSE_LOGIN_LIMIT_KEY", "07".repeat(32));
    let mut child = command
        .args(["--yes", "--auth-stdin", "--output", "json"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    if closed_stdout {
        drop(child.stdout.take());
    }
    let result = child.wait_with_output().unwrap();
    for bytes in [&result.stdout, &result.stderr] {
        assert!(!String::from_utf8_lossy(bytes).contains(PASSWORD));
    }
    let code = result.status.code().unwrap();
    let bytes = if code == 0 {
        assert!(result.stderr.is_empty());
        result.stdout
    } else {
        assert!(result.stdout.is_empty());
        result.stderr
    };
    (code, serde_json::from_slice(&bytes).unwrap())
}
fn input(email: &str) -> Value {
    json!({"email":email,"password":PASSWORD,"reason":"Source-defined operator fixture"})
}
fn succeeds(args: &[&str], email: &str) -> Value {
    let (code, value) = invoke(args, input(email));
    assert_eq!(code, 0, "{value}");
    value["data"].clone()
}
#[tokio::test]
async fn account_cli_authenticates_each_process_shares_http_budgets_and_fails_when_fenced() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    restrict_database(&f).await;
    let (id, email) = actor(&f).await;
    let id = id.to_string();
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM browser_sessions")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    let account = succeeds(&["operator", "account", "show", &id], &email);
    assert_eq!(account["revision"], 0);
    assert_eq!(account["email"], email);
    let changed = succeeds(&["revoke-all", &id, "0"], &email);
    assert_eq!(changed["changed"], true);
    assert_eq!(changed["revision"], 1);
    let stale = invoke(
        &["operator", "account", "revoke-all", &id, "0"],
        input(&email),
    );
    assert_eq!(stale.0, 1);
    assert!(
        stale.1["error"]["message"]
            .as_str()
            .unwrap()
            .contains("changed")
    );
    let wrong = invoke(
        &["account", &id],
        json!({"email":email,"password":"wrong password"}),
    );
    assert_eq!(wrong.0, 1);
    assert!(
        wrong.1["error"]["message"]
            .as_str()
            .unwrap()
            .contains("denied")
    );
    // The exact admission component used by HTTP consumes the fifth attempt.
    let http = SharedLoginAdmission::new(f.limiter(), [7; 32]);
    assert_eq!(http.admit(&email).await, Ok(()));
    let limited = invoke(&["account", &id], input(&email));
    assert_eq!(limited.0, 1);
    assert!(limited.1["data"]["retry_after_ms"].as_u64().unwrap() > 0);
    assert!(matches!(
        http.admit(&email).await,
        Err(AuthError::Limited { .. })
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM browser_sessions")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        before
    );
    let counts:(i64,i64)=sqlx::query_as("SELECT count(*),count(*) FILTER(WHERE actor_id IS NULL) FROM operator_account_audit WHERE target_id=$1 AND database_role='darkhorse_runtime'").bind(Uuid::parse_str(&id).unwrap()).fetch_one(&f.pool).await.unwrap();
    assert_eq!(counts, (4, 1));
    f.fence().await;
    let fenced = invoke(&["account", &id], input(&email));
    assert_eq!(fenced.0, 1);
    assert!(
        fenced.1["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unavailable")
    );
}
#[tokio::test]
async fn account_cli_rechecks_membership_and_credentials_and_commits_all_four_operations() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    restrict_database(&f).await;
    let (id, email) = actor(&f).await;
    let (target, _) = actor(&f).await;
    let target = target.to_string();
    succeeds(&["deactivate", &target, "0"], &email);
    succeeds(&["operator", "account", "reactivate", &target, "1"], &email);
    let row = succeeds(&["account", &target], &email);
    assert_eq!(row["active"], true);
    assert_eq!(row["revision"], 2);
    sqlx::query("DELETE FROM platform_administrators WHERE principal_id=$1")
        .bind(id)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(invoke(&["revoke-all", &target, "2"], input(&email)).0, 1);
    sqlx::query("UPDATE credentials SET revoked=true WHERE principal_id=$1")
        .bind(id)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(invoke(&["account", &target], input(&email)).0, 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT revision FROM principals WHERE id=$1")
            .bind(Uuid::parse_str(&target).unwrap())
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn restricted_runtime_audit_insert_failure_rolls_back_the_actual_cli_operation() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    restrict_database(&f).await;
    let (id, email) = actor(&f).await;
    let target = id.to_string();
    sqlx::query("REVOKE INSERT ON operator_account_audit FROM darkhorse_runtime")
        .execute(&f.pool)
        .await
        .unwrap();
    let read = invoke(&["account", &target], input(&email));
    let change = invoke(&["revoke-all", &target, "0"], input(&email));
    sqlx::query("GRANT INSERT ON operator_account_audit TO darkhorse_runtime")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(read.0, 1);
    assert_eq!(change.0, 1);
    assert!(
        read.1["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unavailable")
    );
    assert!(
        change.1["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unavailable")
    );
    let state:(i64,i64,i64)=sqlx::query_as("SELECT revision,credential_epoch,(SELECT count(*) FROM security_audit WHERE principal_id=$1) FROM principals WHERE id=$1").bind(id).fetch_one(&f.pool).await.unwrap();
    assert_eq!(state, (0, 0, 0));
    succeeds(&["revoke-all", &target, "0"], &email);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM operator_account_audit WHERE target_id=$1 AND database_role='darkhorse_runtime'").bind(id).fetch_one(&f.pool).await.unwrap(),1);
}

#[tokio::test]
async fn operator_database_role_supports_account_commands_and_atomic_audit_failure() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    restrict_database(&f).await;
    let (_, email) = actor(&f).await;
    let (target, _) = actor(&f).await;
    let text = target.to_string();
    for args in [
        vec!["operator", "account", "show", &text],
        vec!["operator", "account", "deactivate", &text, "0"],
        vec!["operator", "account", "reactivate", &text, "1"],
        vec!["operator", "account", "revoke-all", &text, "2"],
    ] {
        let (code, value) = invoke_as("operator", &args, input(&email));
        assert_eq!(code, 0, "{value}");
    }
    sqlx::query("REVOKE INSERT ON operator_account_audit FROM darkhorse_operator")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        invoke_as("operator", &["revoke-all", &text, "3"], input(&email)).0,
        1
    );
    let actual: (i64, i64) =
        sqlx::query_as("SELECT revision,credential_epoch FROM principals WHERE id=$1")
            .bind(target)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(actual, (3, 2));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM operator_account_audit WHERE target_id=$1 AND database_role='darkhorse_operator' AND actor_id IS NOT NULL")
        .bind(target).fetch_one(&f.pool).await.unwrap();
    assert_eq!(count, 4);
}

#[tokio::test]
async fn directory_cli_uses_runtime_grants_fresh_authentication_and_shared_admission() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    restrict_database(&f).await;
    let (id, email) = actor(&f).await;
    let credential = || json!({"email":email,"password":PASSWORD});
    let args = [
        "operator", "account", "list", "--search", &email, "--limit", "1",
    ];
    let (code, page) = invoke(&args, credential());
    assert_eq!(code, 0, "{page}");
    assert_eq!(page["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["data"]["items"][0]["id"], id.to_string());
    assert!(page["data"]["next"].is_null());
    let record:(String,i32,bool)=sqlx::query_as("SELECT database_role,returned_count,searched FROM operator_directory_audit WHERE operation_id=$1")
        .bind(Uuid::parse_str(page["data"]["operation_id"].as_str().unwrap()).unwrap()).fetch_one(&f.pool).await.unwrap();
    assert_eq!(record, ("darkhorse_runtime".into(), 1, true));
    sqlx::query("REVOKE INSERT ON operator_directory_audit FROM darkhorse_runtime")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(invoke(&args, credential()).0, 1);
    sqlx::query("GRANT INSERT ON operator_directory_audit TO darkhorse_runtime")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        invoke(&args, json!({"email":email,"password":"incorrect"})).0,
        1
    );
    sqlx::query("DELETE FROM platform_administrators WHERE principal_id=$1")
        .bind(id)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(invoke(&args, credential()).0, 1);
    let http = SharedLoginAdmission::new(f.limiter(), [7; 32]);
    assert_eq!(http.admit(&email).await, Ok(()));
    let limited = invoke(&args, credential());
    assert_eq!(limited.0, 1);
    assert!(limited.1["data"]["retry_after_ms"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn catalog_cli_uses_current_admin_authority_runtime_grants_and_shared_login_budgets() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    restrict_database(&f).await;
    let (principal, email) = actor(&f).await;
    let app = Uuid::new_v4();
    let client = Uuid::new_v4();
    sqlx::query("INSERT INTO applications(id,name,owner_id,active) VALUES($1,'CLI catalog fixture',$2,true)").bind(app).bind(principal).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO oauth_clients(id,application_id,name,active) VALUES($1,$2,'CLI client fixture',true)").bind(client).bind(app).execute(&f.pool).await.unwrap();
    let input = || json!({"email":email,"password":PASSWORD});
    let (code, value) = invoke(
        &[
            "operator",
            "application",
            "list",
            "--search",
            "CLI catalog",
            "--limit",
            "1",
        ],
        input(),
    );
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["data"]["items"][0]["id"], app.to_string());
    assert_eq!(value["data"]["items"][0].as_object().unwrap().len(), 6);
    let (code, value) = invoke(&["operator", "client", "list", &app.to_string()], input());
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["data"]["items"][0]["id"], client.to_string());
    assert_eq!(value["data"]["items"][0].as_object().unwrap().len(), 5);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM operator_catalog_audit WHERE result='read' AND database_role='darkhorse_runtime'").fetch_one(&f.pool).await.unwrap(),2);
    let (code, value) = invoke(
        &["operator", "client", "list", &Uuid::new_v4().to_string()],
        input(),
    );
    assert_eq!(code, 1);
    assert_eq!(value["error"]["message"], "Catalog target not found.");
    sqlx::query("REVOKE INSERT ON operator_catalog_audit FROM darkhorse_runtime")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(invoke(&["operator", "application", "list"], input()).0, 1);
    sqlx::query("GRANT INSERT ON operator_catalog_audit TO darkhorse_runtime")
        .execute(&f.pool)
        .await
        .unwrap();
    let admission = SharedLoginAdmission::new(f.limiter(), [7; 32]);
    admission.admit(&email).await.unwrap();
    let (code, value) = invoke(&["operator", "application", "list"], input());
    assert_eq!(code, 1);
    assert!(value["data"]["retry_after_ms"].as_u64().unwrap() > 0);
    let (owner, owner_email) = actor(&f).await;
    sqlx::query("UPDATE applications SET revision=revision+1,owner_id=$1 WHERE id=$2")
        .bind(owner)
        .bind(app)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM platform_administrators WHERE principal_id=$1")
        .bind(owner)
        .execute(&f.pool)
        .await
        .unwrap();
    let (code, value) = invoke(
        &["operator", "client", "list", &app.to_string()],
        json!({"email":owner_email,"password":PASSWORD}),
    );
    assert_eq!(code, 1);
    assert_eq!(
        value["error"]["message"],
        "Administrator authentication or authority denied."
    );
    let (code, value) = invoke(
        &["operator", "application", "list"],
        json!({"email":owner_email,"password":"source-only wrong password"}),
    );
    assert_eq!(code, 1);
    assert_eq!(
        value["error"]["message"],
        "Administrator authentication or authority denied."
    );
}

#[tokio::test]
async fn catalog_detail_cli_uses_runtime_grants_without_client_secret_table_access() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    restrict_database(&f).await;
    let (principal, email) = actor(&f).await;
    let app = Uuid::new_v4();
    let client = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO applications(id,name,owner_id,active) VALUES($1,'Detail fixture',$2,true)",
    )
    .bind(app)
    .bind(principal)
    .execute(&f.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO oauth_clients(id,application_id,name,active) VALUES($1,$2,'Detail client',true)").bind(client).bind(app).execute(&f.pool).await.unwrap();
    sqlx::query(
        "INSERT INTO client_redirects VALUES($1,'https://client.example/callback?fixed=1')",
    )
    .bind(client)
    .execute(&f.pool)
    .await
    .unwrap();
    sqlx::query("REVOKE SELECT ON oauth_client_secrets FROM darkhorse_runtime")
        .execute(&f.pool)
        .await
        .unwrap();
    let input = || json!({"email":email,"password":PASSWORD});
    let app_string = app.to_string();
    let client_string = client.to_string();
    for args in [
        vec!["operator", "application", "show", &app_string],
        vec!["operator", "client", "show", &app_string, &client_string],
    ] {
        let (code, value) = invoke(&args, input());
        assert_eq!(code, 0, "{value}");
        assert!(value["data"]["record"]["revision"].is_string());
        assert!(value["data"]["record"].get("secrets").is_none());
    }
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM operator_catalog_detail_audit WHERE result='read' AND database_role='darkhorse_runtime'").fetch_one(&f.pool).await.unwrap(),2);
    let args = ["operator", "client", "show", &app_string, &client_string];
    sqlx::query("REVOKE INSERT ON operator_catalog_detail_audit FROM darkhorse_runtime")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(invoke(&args, input()).0, 1);
    sqlx::query("GRANT INSERT ON operator_catalog_detail_audit TO darkhorse_runtime")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        invoke(
            &args,
            json!({"email":email,"password":"source-only incorrect password"})
        )
        .0,
        1
    );
    let admission = SharedLoginAdmission::new(f.limiter(), [7; 32]);
    admission.admit(&email).await.unwrap();
    let (code, value) = invoke(&args, input());
    assert_eq!(code, 1);
    assert!(value["data"]["retry_after_ms"].as_u64().unwrap() > 0);
    let (owner, owner_email) = actor(&f).await;
    sqlx::query("UPDATE applications SET owner_id=$1,revision=revision+1 WHERE id=$2")
        .bind(owner)
        .bind(app)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM platform_administrators WHERE principal_id=$1")
        .bind(owner)
        .execute(&f.pool)
        .await
        .unwrap();
    let (code, value) = invoke(&args, json!({"email":owner_email,"password":PASSWORD}));
    assert_eq!(code, 1);
    assert_eq!(
        value["error"]["message"],
        "Administrator authentication or authority denied."
    );
}

#[tokio::test]
async fn application_commands_use_runtime_authority_revision_checks_and_transactional_audit() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    restrict_database(&f).await;
    let (owner, email) = actor(&f).await;
    let owner = owner.to_string();
    let create = [
        "operator",
        "application",
        "create",
        "--name",
        "CLI application",
        "--owner",
        &owner,
        "--status",
        "active",
    ];
    let created = succeeds(&create, &email);
    assert_eq!(created.as_object().unwrap().len(), 4);
    assert_eq!(created["revision"], "0");
    let app = created["application_id"].as_str().unwrap();
    let update = [
        "operator",
        "application",
        "update",
        app,
        "0",
        "--name",
        "Updated application",
        "--owner",
        &owner,
        "--status",
        "inactive",
    ];
    let changed = succeeds(&update, &email);
    assert_eq!(changed["revision"], "1");
    let active: bool = sqlx::query_scalar("SELECT active FROM applications WHERE id=$1")
        .bind(Uuid::parse_str(app).unwrap())
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert!(!active);
    assert_eq!(invoke(&update, input(&email)).0, 1);
    assert_eq!(
        invoke(&create, json!({"email":email,"password":PASSWORD})).0,
        2
    );
    assert_eq!(invoke_as("operator", &create, input(&email)).0, 1);
    sqlx::query("REVOKE INSERT ON operator_application_audit FROM darkhorse_runtime")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(invoke(&create, input(&email)).0, 1);
    sqlx::query("GRANT INSERT ON operator_application_audit TO darkhorse_runtime")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM applications WHERE owner_id=$1")
            .bind(Uuid::parse_str(&owner).unwrap())
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
    let (other, other_email) = actor(&f).await;
    sqlx::query("DELETE FROM platform_administrators WHERE principal_id=$1")
        .bind(other)
        .execute(&f.pool)
        .await
        .unwrap();
    let denied = invoke(&create, input(&other_email));
    assert_eq!(denied.0, 1);
    assert!(
        denied.1["error"]["message"]
            .as_str()
            .unwrap()
            .contains("denied")
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM operator_application_audit WHERE actor_id=$1 AND result='written' AND database_role='darkhorse_runtime'").bind(Uuid::parse_str(&owner).unwrap()).fetch_one(&f.pool).await.unwrap(),2);
    let audits: Vec<String> =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM operator_application_audit a")
            .fetch_all(&f.pool)
            .await
            .unwrap();
    assert!(
        audits.iter().all(|a| !a.contains(PASSWORD)
            && !a.contains(&email)
            && !a.contains("CLI application"))
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM browser_sessions WHERE principal_id=$1")
            .bind(Uuid::parse_str(&owner).unwrap())
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn committed_application_creation_with_closed_output_is_not_repeated() {
    let _serial = SERIAL.lock().await;
    let f = Fixture::new().await;
    f.activate().await;
    restrict_database(&f).await;
    let (owner, email) = actor(&f).await;
    let owner_text = owner.to_string();
    let args = [
        "operator",
        "application",
        "create",
        "--name",
        "Output failure fixture",
        "--owner",
        &owner_text,
        "--status",
        "active",
    ];
    let (code, response) = invoke_output("runtime", &args, input(&email), true);
    assert_eq!(code, 74);
    assert_eq!(response["error"]["code"], "output_failed");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM applications WHERE owner_id=$1")
            .bind(owner)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM operator_application_audit WHERE actor_id=$1 AND result='written'"
        )
        .bind(owner)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1
    );
}
