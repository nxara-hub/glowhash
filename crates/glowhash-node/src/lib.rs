//! Node.js bindings for the glowhash crate, built with napi-rs.

use napi::bindgen_prelude::*;
use napi_derive::napi;

fn invalid(e: impl std::fmt::Display) -> Error {
    Error::new(Status::InvalidArg, e.to_string())
}

/// Straight sRGB RGBA pixels, shaped like the browser's `ImageData`.
#[napi(object)]
pub struct Pixels {
    #[napi(ts_type = "Uint8Array")]
    pub data: Buffer,
    pub width: u32,
    pub height: u32,
}

pub struct Encode {
    input: Either<Buffer, Pixels>,
    detail: usize,
}

impl Task for Encode {
    type Output = String;
    type JsValue = String;

    fn compute(&mut self) -> Result<String> {
        match &self.input {
            Either::B(p) => {
                let (w, h) = (p.width as usize, p.height as usize);
                if w == 0 || h == 0 || p.data.len() != w * h * 4 {
                    return Err(invalid("data must be width * height * 4 bytes"));
                }
                Ok(glowhash::encode(w, h, &p.data, self.detail))
            }
            Either::A(file) => {
                let bytes: &[u8] = file;
                // JPEGs decode at reduced size (up to 1/8): most of the time saved on big photos.
                if let Ok(image::ImageFormat::Jpeg) = image::guess_format(bytes) {
                    return glowhash::encode_jpeg(bytes, self.detail).map_err(invalid);
                }
                let img = image::load_from_memory(bytes).map_err(|_| invalid(glowhash::Error::InvalidImage))?.to_rgba8();
                Ok(glowhash::encode(img.width() as usize, img.height() as usize, img.as_raw(), self.detail))
            }
        }
    }

    fn resolve(&mut self, _: Env, hash: String) -> Result<String> {
        Ok(hash)
    }
}

/// Hashes an image file (JPEG, PNG, WebP or TIFF bytes) or raw RGBA pixels, with `detail` cells
/// (2–32, default 16) on the long side. Runs on libuv's thread pool: hash many at once with
/// `Promise.all` to use several cores (`UV_THREADPOOL_SIZE`, default 4).
#[napi(ts_args_type = "input: Uint8Array | Pixels, detail?: number", ts_return_type = "Promise<string>")]
pub fn encode(input: Either<Buffer, Pixels>, detail: Option<u32>) -> AsyncTask<Encode> {
    AsyncTask::new(Encode { input, detail: detail.map_or(glowhash::DETAIL, |d| d as usize) })
}

/// Renders the hash as straight sRGB RGBA pixels. With no size, 32 px on the long side; with
/// one, the other follows the image's aspect ratio.
#[napi]
pub fn decode(hash: String, width: Option<u32>, height: Option<u32>) -> Result<Pixels> {
    let (cols, rows) = glowhash::grid(&hash).map_err(invalid)?;
    let follow = |v: u32, a: usize, b: usize| ((v as f64 * a as f64 / b as f64).round() as u32).max(1);
    let (w, h) = match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, follow(w, rows, cols)),
        (None, Some(h)) => (follow(h, cols, rows), h),
        (None, None) => glowhash::size_for(&hash, 32).map(|(w, h)| (w as u32, h as u32)).map_err(invalid)?,
    };
    if w == 0 || h == 0 {
        return Err(invalid("width and height must be at least 1"));
    }
    let data = glowhash::decode(&hash, w as usize, h as usize).map_err(invalid)?;
    Ok(Pixels { data: data.into(), width: w, height: h })
}
