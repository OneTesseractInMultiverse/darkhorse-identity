use darkhorse_adapters::media::{
    configuration,
    images::Decoder,
    objects::{Storage, fingerprint},
};
use darkhorse_application::media::{Asset, Images, Objects};
use darkhorse_domain::{identity::AssetId, media::Kind};
#[tokio::test]
async fn real_s3_stores_reencoded_images_and_rejects_corruption_or_missing_objects() {
    let settings = configuration::load(envbind::ProcessEnvironment)
        .unwrap()
        .expect("use make test-media for disposable S3 storage");
    let binding = fingerprint(&settings);
    assert_ne!(binding, [0; 32]);
    let store = Storage::new(settings).unwrap();
    let input = image::RgbaImage::from_pixel(600, 300, image::Rgba([3, 4, 5, 255]));
    let mut encoded = std::io::Cursor::new(Vec::new());
    input
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let image = Decoder::default()
        .prepare(Kind::Portrait, "image/png".into(), encoded.into_inner())
        .await
        .unwrap();
    assert_eq!((image.width, image.height), (512, 256));
    let id = AssetId::from_u128(uuid::Uuid::new_v4().as_u128()).unwrap();
    let asset = Asset {
        id,
        bytes: image.bytes.len(),
        digest: image.digest,
    };
    store.put(id, &image.bytes).await.unwrap();
    assert_eq!(store.get(&asset).await.unwrap(), image.bytes);
    store.put(id, b"corrupt").await.unwrap();
    assert!(store.get(&asset).await.is_err());
    store.delete(id).await.unwrap();
    assert!(store.get(&asset).await.is_err());
    store.delete(id).await.unwrap();
}
