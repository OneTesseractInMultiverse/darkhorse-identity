use darkhorse_application::media::{Images, Prepared};
use darkhorse_domain::{
    media::{Kind, MAX_BYTES, MAX_DIMENSION, body_size, dimensions},
    profiles::Error,
};
use image::{ImageEncoder, ImageFormat, ImageReader};
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Write},
    sync::Arc,
};
#[derive(Clone)]
pub struct Decoder {
    slots: Arc<tokio::sync::Semaphore>,
}
impl Default for Decoder {
    fn default() -> Self {
        Self {
            slots: Arc::new(tokio::sync::Semaphore::new(2)),
        }
    }
}
impl Images for Decoder {
    async fn prepare(
        &self,
        kind: Kind,
        content_type: String,
        bytes: Vec<u8>,
    ) -> Result<Prepared, Error> {
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::Unavailable)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            normalize(kind, &content_type, bytes)
        })
        .await
        .map_err(|_| Error::Unavailable)?
    }
}
fn normalize(kind: Kind, content_type: &str, bytes: Vec<u8>) -> Result<Prepared, Error> {
    body_size(bytes.len())?;
    let format = match content_type {
        "image/png" => ImageFormat::Png,
        "image/jpeg" => ImageFormat::Jpeg,
        _ => return Err(Error::Invalid),
    };
    if image::guess_format(&bytes).map_err(|_| Error::Invalid)? != format {
        return Err(Error::Invalid);
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|_| Error::Invalid)?;
    dimensions(decoded.width(), decoded.height())?;
    let image = decoded.thumbnail(kind.edge(), kind.edge()).to_rgba8();
    let (width, height) = image.dimensions();
    let mut output = Bounded(Vec::new());
    image::codecs::png::PngEncoder::new(&mut output)
        .write_image(
            image.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|_| Error::Invalid)?;
    let bytes = output.0;
    let digest = Sha256::digest(&bytes).into();
    Ok(Prepared {
        bytes,
        digest,
        width,
        height,
    })
}
struct Bounded(Vec<u8>);
impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_BYTES - self.0.len() {
            return Err(std::io::Error::other("image size limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[cfg(test)]
#[path = "../../tests/unit/media/images.rs"]
mod tests;
