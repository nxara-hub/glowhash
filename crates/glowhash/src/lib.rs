//! glowhash: blurred image placeholders with real transparency.
//!
//! An image becomes a small grid of colour cells (the true average colour and opacity of each
//! patch), compressed into a short URL-safe hash. Decoding blurs the cells smoothly, which
//! gives BlurHash's soft look while keeping the subject's colours and outline: the glow spreads
//! about one cell past the edge and never wraps around the frame.
//!
//! ```
//! let rgba = vec![255u8; 40 * 30 * 4];
//! let hash = glowhash::encode(40, 30, &rgba, glowhash::DETAIL);
//! let (w, h) = glowhash::size_for(&hash, 32).unwrap();
//! let pixels = glowhash::decode(&hash, w, h).unwrap(); // straight RGBA, ready to save as a PNG
//! # assert_eq!(pixels.len(), w * h * 4);
//! ```

mod codec;
#[cfg(feature = "jpeg")]
mod jpeg;
mod rc;
mod render;

pub use codec::{encode, grid};
#[cfg(feature = "jpeg")]
pub use jpeg::encode_jpeg;
pub use render::decode;

/// Default detail: cells on the image's long side.
pub const DETAIL: usize = 16;
/// Detail is clamped to this range.
pub const DETAIL_RANGE: std::ops::RangeInclusive<usize> = 2..=32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The text is not base64url or is too short to hold a header.
    InvalidHash,
    /// An image file could not be decoded.
    InvalidImage,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            Error::InvalidHash => "invalid glowhash hash",
            Error::InvalidImage => "the image could not be decoded",
        })
    }
}

impl std::error::Error for Error {}

/// A decode size with `long` pixels on the long side, in the image's aspect ratio.
pub fn size_for(hash: &str, long: usize) -> Result<(usize, usize), Error> {
    let (cols, rows) = grid(hash)?;
    let short = |a: usize, b: usize| ((long * a) as f64 / b as f64).round().max(1.0) as usize;
    Ok(if cols >= rows { (long, short(rows, cols)) } else { (short(cols, rows), long) })
}

/// sRGB8 -> linear light, as a table: a powf per pixel would dominate encoding time.
pub(crate) fn srgb_table() -> &'static [f32; 256] {
    static LUT: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        std::array::from_fn(|i| {
            let c = i as f32 / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        })
    })
}

pub(crate) fn srgb_to_linear(v: u8) -> f32 {
    srgb_table()[v as usize]
}

pub(crate) fn linear_to_srgb(v: f32) -> u8 {
    // 4096-entry table: error under one level, no powf per output pixel.
    static LUT: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let lut = LUT.get_or_init(|| {
        (0..4096)
            .map(|i| {
                let c = i as f32 / 4095.0;
                let s = if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
                (s * 255.0 + 0.5) as u8
            })
            .collect()
    });
    lut[(v.clamp(0.0, 1.0) * 4095.0 + 0.5) as usize]
}

const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub(crate) fn from_base64url(text: &str) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.bytes() {
        acc = acc << 6 | B64.iter().position(|&b| b == c).ok_or(Error::InvalidHash)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

pub(crate) fn base64url(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..=c.len() {
            s.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_round_trip() {
        for n in 0..10 {
            let bytes: Vec<u8> = (0..n).map(|i| (i * 97 + 13) as u8).collect();
            assert_eq!(from_base64url(&base64url(&bytes)).unwrap(), bytes);
        }
        assert_eq!(from_base64url("a+b"), Err(Error::InvalidHash));
    }
}
