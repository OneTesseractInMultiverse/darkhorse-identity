use super::*;
#[test]
fn image_budgets_and_upload_lifetimes_are_bounded() {
    for (w, h) in [(0, 1), (1, 0), (2049, 1), (1, 2049)] {
        assert_eq!(dimensions(w, h), Err(Error::Invalid));
    }
    assert_eq!(dimensions(2048, 2048), Ok(()));
    assert_eq!(body_size(0), Err(Error::Invalid));
    assert_eq!(body_size(MAX_BYTES + 1), Err(Error::Invalid));
    assert_eq!(body_size(MAX_BYTES), Ok(()));
    assert_eq!(attachable(100, 100, "pending"), Ok(()));
    assert_eq!(
        attachable(100, 100 + UPLOAD_MS, "pending"),
        Err(Error::Conflict)
    );
    assert_eq!(attachable(100, 99, "pending"), Err(Error::Conflict));
    assert_eq!(attachable(100, 101, "retired"), Err(Error::Conflict));
    assert_eq!(capacity(3), Ok(()));
    assert_eq!(capacity(4), Err(Error::Invalid));
    assert_eq!(Kind::Portrait.edge(), 512);
    assert_eq!(Kind::Logo.edge(), 1024);
    assert_eq!(Kind::Background.edge(), 1920);
}
