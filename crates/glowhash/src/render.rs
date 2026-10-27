//! Cells -> pixels.
//!
//! The cell grid is blurred with a Gaussian of `SOFTNESS` cells, in linear light over
//! premultiplied colour. So the glow reaches about one cell past a subject's outline and colours
//! never smear into transparency. Beyond the grid, an opaque image repeats its edge cells (it
//! stays solid to its frame) and a transparent one is transparent (a subject touching the frame
//! fades there instead of being cut off).

use crate::codec::cells;
use crate::{Error, linear_to_srgb, srgb_to_linear};

/// Blur radius (standard deviation) in cells.
const SOFTNESS: f32 = 0.6;

/// Gaussian weights from `n` cells to `len` output samples. Outside the grid, cells repeat the
/// edge (`extend`) or are transparent (their weight is dropped, so the result fades).
/// `sigma` adds a bilinear upscale's softening (variance 1/6 cell^2) to `SOFTNESS`.
fn weights(n: usize, len: usize, extend: bool) -> Vec<Vec<(usize, f32)>> {
    let sigma = (SOFTNESS * SOFTNESS + 1.0 / 6.0).sqrt();
    let reach = (3.0 * sigma).ceil() as isize;
    (0..len)
        .map(|x| {
            let u = (x as f32 + 0.5) * n as f32 / len as f32 - 0.5;
            let mut taps: Vec<(usize, f32)> = vec![];
            let mut total = 0.0;
            for i in u.floor() as isize - reach..=u.floor() as isize + reach + 1 {
                let w = (-((i as f32 - u).powi(2)) / (2.0 * sigma * sigma)).exp();
                total += w;
                if !extend && (i < 0 || i >= n as isize) {
                    continue;
                }
                let i = i.clamp(0, n as isize - 1) as usize;
                match taps.iter_mut().find(|t| t.0 == i) {
                    Some(t) => t.1 += w,
                    None => taps.push((i, w)),
                }
            }
            taps.iter_mut().for_each(|t| t.1 /= total);
            taps
        })
        .collect()
}

/// Decodes to straight sRGB RGBA8 at any `width` x `height`. Small sizes (32–64 px on the long
/// side, see [`size_for`](crate::size_for)) scaled up look the same and decode fastest.
pub fn decode(hash: &str, width: usize, height: usize) -> Result<Vec<u8>, Error> {
    let (cols, rows, cells) = cells(hash)?;
    let opaque = cells.iter().all(|c| c.rgba()[3] == 255);
    // Premultiplied linear-light cells.
    let prem: Vec<[f32; 4]> = cells
        .iter()
        .map(|c| {
            let p = c.rgba();
            let a = p[3] as f32 / 255.0;
            [srgb_to_linear(p[0]) * a, srgb_to_linear(p[1]) * a, srgb_to_linear(p[2]) * a, a]
        })
        .collect();
    let (wx, wy) = (weights(cols, width, opaque), weights(rows, height, opaque));
    // Horizontal pass: rows x width.
    let mut mid = vec![[0f32; 4]; rows * width];
    for r in 0..rows {
        for (x, taps) in wx.iter().enumerate() {
            let o = &mut mid[r * width + x];
            for &(i, t) in taps {
                let p = prem[r * cols + i];
                (0..4).for_each(|k| o[k] += p[k] * t);
            }
        }
    }
    let mut out = Vec::with_capacity(width * height * 4);
    for taps in &wy {
        for x in 0..width {
            let mut p = [0f32; 4];
            for &(r, t) in taps {
                let m = mid[r * width + x];
                (0..4).for_each(|k| p[k] += m[k] * t);
            }
            let a = p[3].clamp(0.0, 1.0);
            let s = |v: f32| if a > 0.0 { linear_to_srgb(v / a) } else { 0 };
            out.extend_from_slice(&[s(p[0]), s(p[1]), s(p[2]), (a * 255.0 + 0.5) as u8]);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode;

    #[test]
    fn decode_keeps_colour_inside_and_clears_outside() {
        let (w, h) = (120, 120);
        // A red disc on transparency.
        let px: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = ((i % w) as i32 - 60, (i / w) as i32 - 60);
                [220, 30, 40, if x * x + y * y < 35 * 35 { 255 } else { 0 }]
            })
            .collect();
        let hash = encode(w, h, &px, 16);
        let out = decode(&hash, 64, 64).unwrap();
        let at = |x: usize, y: usize| &out[(y * 64 + x) * 4..][..4];
        let c = at(32, 32);
        assert!(c[3] == 255 && c[0] > 200 && c[1] < 50, "centre {c:?}");
        // Corners are far outside the disc: fully clear, no glow.
        for (x, y) in [(0, 0), (63, 0), (0, 63), (63, 63)] {
            assert_eq!(at(x, y)[3], 0, "corner {x},{y}");
        }
        // Just inside the outline the colour is still red, not darkened by transparency.
        let edge = at(32, 32 - 17);
        assert!(edge[3] > 0 && edge[0] > 180, "edge {edge:?}");
    }

    #[test]
    fn transparent_subject_touching_the_frame_fades_there() {
        // An opaque bar across the full width of an otherwise transparent image.
        let (w, h) = (80, 60);
        let px: Vec<u8> = (0..w * h).flat_map(|i| [40, 160, 90, if (15..45).contains(&(i / w)) { 255 } else { 0 }]).collect();
        let out = decode(&encode(w, h, &px, 16), 80, 60).unwrap();
        let alpha = |x: usize| out[(30 * 80 + x) * 4 + 3];
        assert!(alpha(40) > 240 && alpha(0) < alpha(40) && alpha(0) > 60, "{} {}", alpha(0), alpha(40));
    }

    #[test]
    fn opaque_stays_opaque_to_the_frame() {
        let px: Vec<u8> = (0..50 * 40).flat_map(|i| [(i % 50 * 5) as u8, 120, 60, 255]).collect();
        let out = decode(&encode(50, 40, &px, 12), 30, 24).unwrap();
        assert!(out.chunks(4).all(|p| p[3] == 255));
    }
}
