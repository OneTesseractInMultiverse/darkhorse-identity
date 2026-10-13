use super::*;

const MAX_SEED_BYTES: usize = 4096;
const SAMPLE_POSITIONS: usize = 32;
const MAX_CASES: usize = 260;
const KINDS: [Kind; 3] = [Kind::Portrait, Kind::Logo, Kind::Background];

fn seed(format: ImageFormat) -> Vec<u8> {
    let pixels = image::RgbImage::from_fn(64, 3, |x, y| {
        image::Rgb([(x * 13) as u8, (y * 23) as u8, ((x + y) * 7) as u8])
    });
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(pixels)
        .write_to(&mut output, format)
        .unwrap();
    let bytes = output.into_inner();
    assert!(bytes.len() <= MAX_SEED_BYTES);
    bytes
}

// Fixed sampling includes the first and last bytes, without an external corpus,
// random seed, or an unbounded number of mutation attempts.
fn corpus(seed: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut cases = vec![("original".to_owned(), seed.to_vec())];
    for end in 0..=seed.len().min(64) {
        cases.push((format!("prefix-{end}"), seed[..end].to_vec()));
    }
    for sample in 0..SAMPLE_POSITIONS {
        let offset = sample * (seed.len() - 1) / (SAMPLE_POSITIONS - 1);
        for mask in [0x01, 0x80, 0xff] {
            let mut bytes = seed.to_vec();
            bytes[offset] ^= mask;
            cases.push((format!("xor-{offset}-{mask:02x}"), bytes));
        }
        let end = (offset + 8).min(seed.len());
        for value in [0x00, 0xff] {
            let mut bytes = seed.to_vec();
            bytes[offset..end].fill(value);
            cases.push((format!("fill-{offset}-{value:02x}"), bytes));
        }
        let mut bytes = seed.to_vec();
        bytes.drain(offset..end);
        cases.push((format!("delete-{offset}-{end}"), bytes));
    }
    let mut appended = seed.to_vec();
    appended.extend(0..32);
    cases.push(("trailing-bytes".to_owned(), appended));
    assert!(cases.len() <= MAX_CASES);
    assert!(
        cases
            .iter()
            .all(|(_, bytes)| bytes.len() <= MAX_SEED_BYTES + 32)
    );
    cases
}

fn assert_output(output: &Prepared, kind: Kind, case: &str) {
    assert!(!output.bytes.is_empty(), "{case}");
    assert!(output.bytes.len() <= MAX_BYTES, "{case}");
    assert_eq!(
        image::guess_format(&output.bytes).unwrap(),
        ImageFormat::Png,
        "{case}"
    );
    let pixels = image::load_from_memory_with_format(&output.bytes, ImageFormat::Png)
        .unwrap_or_else(|error| panic!("{case}: undecodable output: {error}"));
    assert_eq!(pixels.width(), output.width, "{case}");
    assert_eq!(pixels.height(), output.height, "{case}");
    assert!((1..=kind.edge()).contains(&pixels.width()), "{case}");
    assert!((1..=kind.edge()).contains(&pixels.height()), "{case}");
    let digest: [u8; 32] = Sha256::digest(&output.bytes).into();
    assert_eq!(output.digest, digest, "{case}");
}

fn check_mutations(format: ImageFormat, mime: &str) {
    let cases = corpus(&seed(format));
    for kind in KINDS {
        let mut accepted = 0;
        let mut rejected = 0;
        for (label, bytes) in &cases {
            let case = format!("{format:?}/{kind:?}/{label}");
            let result = std::panic::catch_unwind(|| normalize(kind, mime, bytes.clone()))
                .unwrap_or_else(|_| panic!("{case}: decoder panicked"));
            match result {
                Ok(output) => {
                    assert_output(&output, kind, &case);
                    accepted += 1;
                }
                Err(Error::Invalid) => rejected += 1,
                Err(error) => panic!("{case}: unexpected error: {error:?}"),
            }
        }
        assert!(accepted > 0, "{format:?}/{kind:?}: no accepted inputs");
        assert!(rejected > 0, "{format:?}/{kind:?}: no rejected inputs");
    }
}

#[test]
fn generated_seeds_resize_successfully_for_every_media_kind() {
    for (format, mime) in [
        (ImageFormat::Png, "image/png"),
        (ImageFormat::Jpeg, "image/jpeg"),
    ] {
        let bytes = seed(format);
        for (kind, dimensions) in [
            (Kind::Portrait, (512, 24)),
            (Kind::Logo, (1024, 48)),
            (Kind::Background, (1920, 90)),
        ] {
            let case = format!("{format:?}/{kind:?}/seed");
            let output = normalize(kind, mime, bytes.clone()).unwrap();
            assert_output(&output, kind, &case);
            assert_eq!((output.width, output.height), dimensions, "{case}");
        }
    }
}

#[test]
fn png_mutations_reject_or_produce_bounded_decodable_output_without_panicking() {
    check_mutations(ImageFormat::Png, "image/png");
}

#[test]
fn jpeg_mutations_reject_or_produce_bounded_decodable_output_without_panicking() {
    check_mutations(ImageFormat::Jpeg, "image/jpeg");
}

#[test]
fn damaged_signatures_and_forged_media_types_are_invalid_for_every_media_kind() {
    for (format, mime, forged) in [
        (ImageFormat::Png, "image/png", "image/jpeg"),
        (ImageFormat::Jpeg, "image/jpeg", "image/png"),
    ] {
        let bytes = seed(format);
        let mut damaged = bytes.clone();
        damaged[..8].fill(0);
        for kind in KINDS {
            for (content_type, input) in [
                (mime, Vec::new()),
                (mime, bytes[..8].to_vec()),
                (mime, damaged.clone()),
                (forged, bytes.clone()),
                ("image/webp", bytes.clone()),
            ] {
                assert!(matches!(
                    normalize(kind, content_type, input),
                    Err(Error::Invalid)
                ));
            }
        }
    }
}
