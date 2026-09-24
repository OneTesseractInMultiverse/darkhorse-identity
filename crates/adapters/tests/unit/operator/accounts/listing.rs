use super::*;
use crate::operator::output::{Format, OUTPUT_LIMIT, render};
use darkhorse_application::admin_directory::User;
use darkhorse_domain::identity::PrincipalId;
#[test]
fn largest_valid_page_fits_safe_output_and_exposes_only_directory_fields() {
    let principal = PrincipalId::from_u128(1).unwrap();
    let page = Page {
        actor: principal,
        next: Some(principal),
        items: (0..25)
            .map(|_| User {
                id: principal,
                email: "e".repeat(254),
                first_name: "\u{202e}".repeat(100),
                last_name: "\u{2066}".repeat(100),
                status: AccountStatus::Active,
                administrator: true,
                email_verified: true,
                revision: i64::MAX as u64,
            })
            .collect(),
    };
    let record = output(OperationId::from_u128(1).unwrap(), page);
    for format in [Format::Human, Format::Json] {
        let bytes = render(&record, format).unwrap();
        assert!(bytes.len() < OUTPUT_LIMIT);
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains('\u{202e}'));
        let _: serde_json::Value = serde_json::from_str(&text).unwrap();
    }
    let row = record.data["items"][0].as_object().unwrap();
    assert_eq!(row.len(), 8);
    for key in [
        "id",
        "email",
        "first_name",
        "last_name",
        "active",
        "administrator",
        "email_verified",
        "revision",
    ] {
        assert!(row.contains_key(key));
    }
}
#[test]
fn empty_page_has_no_continuation() {
    let record = output(
        OperationId::from_u128(1).unwrap(),
        Page {
            actor: PrincipalId::from_u128(1).unwrap(),
            next: None,
            items: vec![],
        },
    );
    assert_eq!(record.data["items"], serde_json::json!([]));
    assert!(record.data["next"].is_null());
}
