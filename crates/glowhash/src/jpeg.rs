//! JPEG files straight to a hash, decoding at reduced size.
//!
//! A hash needs at most 32 cells a side, so decoding a 12–48 MP JPEG in full is mostly wasted
//! work. JPEG can decode at 1/2, 1/4 or 1/8 size cheaply (a smaller inverse DCT per block), so
//! `encode_jpeg` asks for the smallest of those that still gives 16 pixels per cell on the long
//! side: plenty for the cell averages, and up to 64x less pixel data.

use crate::{DETAIL_RANGE, Error, encode};

/// Pixels kept per cell on the long side.
const PER_CELL: usize = 16;

/// Hashes a JPEG file (baseline or progressive) with `detail` cells on the long side, decoding
/// it at the smallest of 1/1, 1/2, 1/4 and 1/8 size that keeps 16 px per cell on the long side
/// (256 px at the default detail). Worth it from about 2 megapixels; for small JPEGs a faster
/// full-size decoder plus [`encode`] can win. EXIF orientation is not applied, as with most
/// image decoders.
pub fn encode_jpeg(jpeg: &[u8], detail: usize) -> Result<String, Error> {
    let mut decoder = jpeg_decoder::Decoder::new(jpeg);
    decoder.read_info().map_err(|_| Error::InvalidImage)?;
    let info = decoder.info().ok_or(Error::InvalidImage)?;
    let (w, h) = (info.width as usize, info.height as usize);
    let long = detail.clamp(*DETAIL_RANGE.start(), *DETAIL_RANGE.end()) * PER_CELL;
    let s = (long as f64 / w.max(h) as f64).min(1.0);
    let want = |v: usize| ((v as f64 * s).ceil() as u16).max(1);
    let (sw, sh) = decoder.scale(want(w), want(h)).map_err(|_| Error::InvalidImage)?;
    let px = decoder.decode().map_err(|_| Error::InvalidImage)?;
    let (sw, sh) = (sw as usize, sh as usize);
    let rgba: Vec<u8> = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => px.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        jpeg_decoder::PixelFormat::L8 => px.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        jpeg_decoder::PixelFormat::L16 => px.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], 255]).collect(),
        // Adobe CMYK JPEGs store inverted values: channel * K / 255 gives RGB.
        jpeg_decoder::PixelFormat::CMYK32 => px
            .chunks_exact(4)
            .flat_map(|p| {
                let k = p[3] as u16;
                [(p[0] as u16 * k / 255) as u8, (p[1] as u16 * k / 255) as u8, (p[2] as u16 * k / 255) as u8, 255]
            })
            .collect(),
    };
    if rgba.len() != sw * sh * 4 {
        return Err(Error::InvalidImage);
    }
    Ok(encode(sw, sh, &rgba, detail))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg(w: u32, h: u32) -> (Vec<u8>, image::RgbImage) {
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x * 255 / w) as u8, (y * 255 / h) as u8, if (x / 40 + y / 40) % 2 == 0 { 200 } else { 40 }])
        });
        let mut bytes = vec![];
        image::DynamicImage::ImageRgb8(img.clone()).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Jpeg).unwrap();
        (bytes, img)
    }

    #[test]
    fn matches_a_full_decode_closely() {
        let (bytes, _) = jpeg(3000, 2000);
        let fast = encode_jpeg(&bytes, 16).unwrap();
        let full = image::load_from_memory(&bytes).unwrap().to_rgba8();
        let slow = encode(3000, 2000, full.as_raw(), 16);
        assert_eq!(crate::grid(&fast), crate::grid(&slow));
        // The two decodes differ a little (scaled IDCT vs full), so compare renderings.
        let (a, b) = (crate::decode(&fast, 24, 16).unwrap(), crate::decode(&slow, 24, 16).unwrap());
        let worst = a.iter().zip(&b).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
        assert!(worst <= 6, "renderings differ by up to {worst} levels");
    }

    #[test]
    fn small_and_bad_input() {
        let (bytes, _) = jpeg(40, 30);
        assert_eq!(crate::grid(&encode_jpeg(&bytes, 8).unwrap()).unwrap(), (8, 6));
        assert_eq!(encode_jpeg(b"not a jpeg", 16), Err(Error::InvalidImage));
    }
}
