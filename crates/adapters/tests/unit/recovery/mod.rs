use super::{begin_at, serving_at};
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{Value, json};
use std::{
    fs::{self, DirBuilder, File},
    os::unix::fs::{DirBuilderExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const DEPLOYMENT: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const OPERATION: &str = "bbbbbbbb-bbbb-4bbb-9bbb-bbbbbbbbbbbb";
const ISSUER: &str = "https://id.example:8443";
const FINGERPRINT: &str = "fe812c12f3ab4ce6ac5db69ac352f906cb1b11ef43fb33e252ef7ff552263889";
const ARCHIVE: &str = "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd";

struct Fixture {
    base: PathBuf,
    control: PathBuf,
    key: Ed25519KeyPair,
}

impl Fixture {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!("darkhorse-recovery-test-{}", Uuid::new_v4()));
        DirBuilder::new().mode(0o700).create(&base).unwrap();
        let control = base.join("recovery");
        fs::create_dir(&control).unwrap();
        fs::set_permissions(&control, fs::Permissions::from_mode(0o755)).unwrap();
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        for name in ["deployment", "serving"] {
            control_file(control.join(name), format!("{DEPLOYMENT}\n{ISSUER}\n"));
        }
        control_file(control.join("administrator"), key.public_key().as_ref());
        Self { base, control, key }
    }

    fn serving(&self) -> Result<Option<File>, &'static str> {
        serving_at(&self.control, false, Some(ISSUER))
    }

    fn begin(&self, wire: &Value) -> Result<(), &'static str> {
        begin_at(&self.control, Some(ISSUER), wire.to_string().as_bytes())
    }

    fn envelope(&self, request: &str) -> Value {
        json!({
            "request": request,
            "signature": STANDARD.encode(self.key.sign(request.as_bytes()).as_ref()),
            "archive_sha256": ARCHIVE
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.base).unwrap();
    }
}

fn control_file(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) {
    let path = path.as_ref();
    if path.exists() {
        fs::remove_file(path).unwrap();
    }
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o444)).unwrap();
}

fn request() -> Value {
    json!({
        "action": "darkhorse.recovery.begin.v1", "deployment": DEPLOYMENT,
        "issuer": ISSUER, "operation": OPERATION, "archive_sha256": ARCHIVE
    })
}

#[test]
fn only_an_absent_optional_directory_can_skip_the_lease() {
    let fixture = Fixture::new();
    let missing = fixture.base.join("missing");
    assert!(serving_at(&missing, false, None).unwrap().is_none());
    assert!(serving_at(&missing, true, None).is_err());
    assert!(serving_at(&fixture.control, false, None).is_err());
    symlink(&missing, fixture.base.join("dangling")).unwrap();
    assert!(serving_at(&fixture.base.join("dangling"), false, Some(ISSUER)).is_err());
    assert!(serving_at(&fixture.control, false, Some("https://other.example")).is_err());
    assert!(fixture.serving().unwrap().is_some());
}

#[test]
fn shared_leases_exclude_quarantine_and_an_exclusive_writer_excludes_serving() {
    let fixture = Fixture::new();
    let wire = fixture.envelope(&request().to_string());
    let first = fixture.serving().unwrap().unwrap();
    let second = fixture.serving().unwrap().unwrap();
    assert!(fixture.begin(&wire).is_err());
    drop(first);
    assert!(fixture.begin(&wire).is_err());
    drop(second);
    let exclusive = File::open(&fixture.control).unwrap();
    exclusive.try_lock().unwrap();
    assert!(fixture.serving().is_err());
    assert!(fixture.begin(&wire).is_err());
    drop(exclusive);
    fixture.begin(&wire).unwrap();
}

#[test]
fn begin_consumes_the_permit_and_rechecks_proof_and_receipt_on_retry() {
    let fixture = Fixture::new();
    let request = format!("{}\n", request());
    let wire = fixture.envelope(&request);
    fixture.begin(&wire).unwrap();
    assert!(!fixture.control.join("serving").exists());
    assert!(fixture.serving().is_err());
    let receipt = fixture
        .control
        .join(format!("quarantine-{OPERATION}-{ARCHIVE}-{FINGERPRINT}"));
    assert_eq!(
        fs::read_to_string(&receipt).unwrap(),
        format!("{DEPLOYMENT}\n{ISSUER}\n")
    );
    fixture.begin(&wire).unwrap();
    let mut changed = wire.clone();
    changed["request"] = Value::from(request.trim_end());
    assert!(fixture.begin(&changed).is_err());
    let other = fixture.envelope(&request.replace(OPERATION, DEPLOYMENT));
    assert!(fixture.begin(&other).is_err());
    let different_archive = "0".repeat(64);
    let mut changed = fixture.envelope(&request.replace(ARCHIVE, &different_archive));
    changed["archive_sha256"] = Value::from(different_archive);
    assert!(fixture.begin(&changed).is_err());
    let other_key = Ed25519KeyPair::from_seed_unchecked(&[9; 32]).unwrap();
    let mut resigned = wire.clone();
    resigned["signature"] =
        Value::from(STANDARD.encode(other_key.sign(request.as_bytes()).as_ref()));
    let pin = fixture.control.join("administrator");
    control_file(&pin, other_key.public_key().as_ref());
    assert!(fixture.begin(&resigned).is_err());
    control_file(&pin, fixture.key.public_key().as_ref());
    control_file(
        fixture.control.join("serving"),
        format!("{DEPLOYMENT}\n{ISSUER}\n"),
    );
    assert!(fixture.serving().is_err());
    assert!(fixture.begin(&wire).is_err());
    fs::remove_file(fixture.control.join("serving")).unwrap();
    control_file(&receipt, format!("{OPERATION}\n{ISSUER}\n"));
    assert!(fixture.begin(&wire).is_err());
}

#[test]
fn unsigned_mismatched_or_malformed_requests_leave_the_permit_intact() {
    let fixture = Fixture::new();
    for (field, value) in [
        ("action", "darkhorse.recovery.release.v1"),
        ("deployment", OPERATION),
        ("issuer", "https://other.example"),
        ("operation", "bbbbbbbb-bbbb-4bbb-7bbb-bbbbbbbbbbbb"),
        ("operation", "bbbbbbbb-bbbb-1bbb-9bbb-bbbbbbbbbbbb"),
        ("operation", &OPERATION.to_uppercase()),
        ("archive_sha256", &ARCHIVE.to_uppercase()),
        ("archive_sha256", "abab"),
        ("extra", "unexpected"),
    ] {
        let mut modified = request();
        modified[field] = Value::from(value);
        assert!(
            fixture
                .begin(&fixture.envelope(&modified.to_string()))
                .is_err(),
            "{field}"
        );
    }
    let request = request().to_string();
    let duplicate = request.replacen('{', "{\"action\":\"darkhorse.recovery.begin.v1\",", 1);
    assert!(fixture.begin(&fixture.envelope(&duplicate)).is_err());
    let mut wrong = fixture.envelope(&request);
    wrong["archive_sha256"] = Value::from("0".repeat(64));
    assert!(fixture.begin(&wrong).is_err());
    let mut wrong = fixture.envelope(&request);
    wrong["request"] = Value::from(format!("{request}\n"));
    assert!(fixture.begin(&wrong).is_err());
    let mut wrong = fixture.envelope(&request);
    wrong["unexpected"] = Value::from(true);
    assert!(fixture.begin(&wrong).is_err());
    let mut wrong = fixture.envelope(&request);
    wrong["signature"] = Value::from("!");
    assert!(fixture.begin(&wrong).is_err());
    let mut oversized = fixture.envelope(&request).to_string();
    oversized.push_str(&" ".repeat(16 * 1024));
    assert!(begin_at(&fixture.control, Some(ISSUER), oversized.as_bytes()).is_err());
    assert!(fixture.serving().unwrap().is_some());
}

#[test]
fn enrollment_is_optional_for_serving_and_mandatory_for_recovery() {
    let fixture = Fixture::new();
    let admin = fixture.control.join("administrator");
    fs::remove_file(&admin).unwrap();
    assert!(fixture.serving().unwrap().is_some());
    let wire = fixture.envelope(&request().to_string());
    assert!(fixture.begin(&wire).is_err());
    let other = Ed25519KeyPair::from_seed_unchecked(&[9; 32]).unwrap();
    control_file(&admin, other.public_key().as_ref());
    assert!(fixture.begin(&wire).is_err());
    control_file(&admin, [1; 33]);
    assert!(fixture.serving().is_err());
    assert!(fixture.begin(&wire).is_err());
}

#[test]
fn ambiguous_unknown_or_unsafe_control_files_deny_startup_and_fencing() {
    let fixture = Fixture::new();
    let wire = fixture.envelope(&request().to_string());
    let serving = fixture.control.join("serving");
    for name in ["quarantine-incomplete", "unrecognized"] {
        let path = fixture.control.join(name);
        control_file(&path, format!("{DEPLOYMENT}\n{ISSUER}\n"));
        assert!(fixture.serving().is_err());
        assert!(fixture.begin(&wire).is_err());
        fs::remove_file(path).unwrap();
    }
    fs::remove_file(&serving).unwrap();
    assert!(fixture.serving().is_err());
    assert!(fixture.begin(&wire).is_err());
    symlink("deployment", &serving).unwrap();
    assert!(fixture.serving().is_err());
    assert!(fixture.begin(&wire).is_err());
    fs::remove_file(&serving).unwrap();
    fs::create_dir(&serving).unwrap();
    assert!(fixture.serving().is_err());
    assert!(fixture.begin(&wire).is_err());
}

#[test]
fn writable_or_multiply_linked_records_are_not_authority() {
    let fixture = Fixture::new();
    let wire = fixture.envelope(&request().to_string());
    let linked = fixture.base.join("second-link");
    for name in ["deployment", "serving", "administrator"] {
        let path = fixture.control.join(name);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(fixture.serving().is_err(), "{name} writable");
        assert!(fixture.begin(&wire).is_err(), "{name} writable");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        fs::hard_link(&path, &linked).unwrap();
        assert!(fixture.serving().is_err(), "{name} linked");
        assert!(fixture.begin(&wire).is_err(), "{name} linked");
        fs::remove_file(&linked).unwrap();
    }
    assert!(fixture.serving().unwrap().is_some());
    fixture.begin(&wire).unwrap();
    let receipt = fixture
        .control
        .join(format!("quarantine-{OPERATION}-{ARCHIVE}-{FINGERPRINT}"));
    fs::hard_link(&receipt, linked).unwrap();
    assert!(fixture.begin(&wire).is_err());
}

#[test]
fn canonical_local_identity_is_required_even_if_both_files_match() {
    let fixture = Fixture::new();
    for (deployment, issuer) in [
        (DEPLOYMENT, "http://id.example"),
        (DEPLOYMENT, "https://id.example/"),
        (DEPLOYMENT, "https://ID.EXAMPLE"),
        (DEPLOYMENT, "https://id.example:443"),
        (DEPLOYMENT, "https://id.example:0"),
        (DEPLOYMENT, "https://user@id.example"),
        (DEPLOYMENT, "https://id.example/?query"),
        (DEPLOYMENT, "https://id.example/#frag"),
        ("aaaaaaaa-aaaa-1aaa-8aaa-aaaaaaaaaaaa", ISSUER),
    ] {
        for name in ["deployment", "serving"] {
            control_file(
                fixture.control.join(name),
                format!("{deployment}\n{issuer}\n"),
            );
        }
        assert!(
            serving_at(&fixture.control, true, Some(issuer)).is_err(),
            "{deployment} {issuer}"
        );
    }
}
