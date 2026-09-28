use super::*;
fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([3, 4, 5, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}
fn jpeg(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbImage::from_pixel(width, height, image::Rgb([3, 4, 5]));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .unwrap();
    out.into_inner()
}
#[test]
fn image_content_is_decoded_and_rewritten_with_strict_bounds() {
    let input = png(800, 400);
    let mut tainted = input.clone();
    tainted.extend(b"<script>bad()</script>");
    let result = normalize(Kind::Portrait, "image/png", tainted).unwrap();
    assert_eq!((result.width, result.height), (512, 256));
    assert!(!result.bytes.windows(8).any(|s| s == b"<script>"));
    assert_eq!(
        image::guess_format(&result.bytes).unwrap(),
        image::ImageFormat::Png
    );
    for (mime, body) in [
        ("image/jpeg", input.clone()),
        ("image/svg+xml", b"<svg/>".to_vec()),
        ("image/png", b"not an image".to_vec()),
        ("image/png", png(2049, 1)),
        ("image/png", vec![0; MAX_BYTES + 1]),
    ] {
        assert!(normalize(Kind::Logo, mime, body).is_err());
    }
    let mut writer = Bounded(Vec::new());
    assert!(std::io::Write::write(&mut writer, &vec![0; MAX_BYTES + 1]).is_err());
    assert!(std::io::Write::flush(&mut writer).is_ok());
}

#[test]
fn jpeg_payload_is_rewritten_as_png_without_appended_content() {
    let mut input = jpeg(1200, 400);
    input.extend(b"untrusted trailing content");
    let result = normalize(Kind::Logo, "image/jpeg", input).unwrap();

    assert_eq!((result.width, result.height), (1024, 341));
    assert_eq!(
        image::guess_format(&result.bytes).unwrap(),
        image::ImageFormat::Png
    );
    assert!(
        !result
            .bytes
            .windows(b"untrusted trailing content".len())
            .any(|window| window == b"untrusted trailing content")
    );
}

#[test]
fn decoder_rejects_empty_and_truncated_image_bodies() {
    let valid = png(32, 16);
    for body in [
        Vec::new(),
        valid[..8].to_vec(),
        valid[..valid.len() / 2].to_vec(),
    ] {
        assert!(normalize(Kind::Portrait, "image/png", body).is_err());
    }
}

#[test]
fn decoder_accepts_the_maximum_declared_dimension() {
    let result = normalize(Kind::Background, "image/png", png(MAX_DIMENSION, 1)).unwrap();

    assert_eq!((result.width, result.height), (1920, 1));
}
