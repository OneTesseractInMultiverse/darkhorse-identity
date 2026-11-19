use super::*;

#[test]
fn responses_require_complete_objects_and_explicit_mutation_success() {
    assert!(read_status(StatusCode::OK, Some(3), 3).is_ok());
    for (status, size) in [
        (StatusCode::OK, None),
        (StatusCode::OK, Some(2)),
        (StatusCode::OK, Some(4)),
        (StatusCode::PARTIAL_CONTENT, Some(3)),
        (StatusCode::FOUND, Some(3)),
        (StatusCode::INTERNAL_SERVER_ERROR, Some(3)),
    ] {
        assert_eq!(read_status(status, size, 3), Err(Error::Unavailable));
    }
    assert!(mutation_status(StatusCode::OK, false).is_ok());
    assert!(mutation_status(StatusCode::NO_CONTENT, true).is_ok());
    assert!(mutation_status(StatusCode::NOT_FOUND, true).is_ok());
    for status in [
        StatusCode::NOT_FOUND,
        StatusCode::FOUND,
        StatusCode::FORBIDDEN,
        StatusCode::SERVICE_UNAVAILABLE,
    ] {
        assert_eq!(mutation_status(status, false), Err(Error::Unavailable));
    }
    assert_eq!(
        mutation_status(StatusCode::FORBIDDEN, true),
        Err(Error::Unavailable)
    );
}

#[tokio::test]
async fn body_stream_is_bounded_before_accumulation_and_errors_propagate() {
    let chunks = [
        Ok::<_, ()>(Bytes::from_static(b"ab")),
        Ok(Bytes::from_static(b"c")),
    ];
    assert_eq!(
        read_body(futures_util::stream::iter(chunks.clone()), 3)
            .await
            .unwrap(),
        b"abc"
    );
    assert_eq!(
        read_body(futures_util::stream::iter(chunks), 2).await,
        Err(Error::Unavailable)
    );
    let chunks = [Ok(Bytes::from_static(b"ab")), Err(())];
    assert_eq!(
        read_body(futures_util::stream::iter(chunks), 3).await,
        Err(Error::Unavailable)
    );
    // The cache decorator additionally rejects truncated bodies and incorrect digests.
    let chunks = [Ok::<_, ()>(Bytes::from_static(b"ab"))];
    assert_eq!(
        read_body(futures_util::stream::iter(chunks), 3)
            .await
            .unwrap(),
        b"ab"
    );
    let mut bytes = vec![1, 2];
    assert_eq!(append(&mut bytes, &[3, 4], 3), Err(Error::Unavailable));
    assert_eq!(bytes, [1, 2]);
}
