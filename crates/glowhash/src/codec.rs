//! Image <-> cell grid <-> bytes.
//!
//! Cells are area averages in linear light, weighted by alpha, so transparent pixels add
//! nothing to a cell's colour. Each cell is stored as straight sRGB in quantised YCoCg plus a
//! 4-bit alpha, predicted from its neighbours and range-coded. The byte layout is in
//! spec/FORMAT.md.

use crate::rc::{RcDec, RcEnc};
use crate::{DETAIL_RANGE, Error, base64url, from_base64url, linear_to_srgb, srgb_table};

const VERSION: u8 = 1;
const ALPHA_LEVELS: i32 = 15; // alpha 0..=15
pub(crate) const Y_STEP: i32 = 4; // Y 0..=64 in steps of 4
pub(crate) const C_STEP: i32 = 8; // Co, Cg -32..=32 in steps of 8

/// A decoded cell: quantised alpha 0..=15 and colour (Y, Co, Cg) in quantised units.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(crate) struct Cell {
    pub(crate) a: i32,
    pub(crate) c: [i32; 3],
}

impl Cell {
    /// Straight sRGB8 RGBA.
    pub(crate) fn rgba(&self) -> [u8; 4] {
        let (y, co, cg) = ((self.c[0] * Y_STEP) as f32, (self.c[1] * C_STEP) as f32, (self.c[2] * C_STEP) as f32);
        let g = y + cg / 2.0;
        let t = y - cg / 2.0;
        let ch = |v: f32| v.round().clamp(0.0, 255.0) as u8;
        [ch(t + co / 2.0), ch(g), ch(t - co / 2.0), (self.a * 255 / ALPHA_LEVELS) as u8]
    }
}

/// Grid for an image: `detail` cells on the long side.
fn grid_for(w: usize, h: usize, detail: usize) -> (usize, usize) {
    let short = |a: usize, b: usize| ((detail * a) as f64 / b as f64).round().clamp(1.0, detail as f64) as usize;
    if w >= h { (detail, short(h, w)) } else { (short(w, h), detail) }
}

/// Area-average cells in one pass: alpha-weighted linear colour and alpha per cell.
fn cells_of(w: usize, h: usize, rgba: &[u8], cols: usize, rows: usize) -> Vec<Cell> {
    let mut sum = vec![[0f32; 5]; cols * rows]; // r, g, b weighted by alpha; alpha; pixel count
    // Pixel x belongs to column x * cols / w, i.e. columns start at ceil(c * w / cols).
    let starts: Vec<usize> = (0..=cols).map(|c| (c * w).div_ceil(cols)).collect();
    let lin = srgb_table();
    for (y, line) in rgba.chunks_exact(w * 4).enumerate() {
        let row = &mut sum[(y * rows / h) * cols..][..cols];
        for (c, s) in row.iter_mut().enumerate() {
            // Sum each run in locals and touch memory once per cell per row: accumulating
            // in place makes every pixel wait on the previous one.
            let (mut r, mut g, mut b, mut a) = (0f32, 0f32, 0f32, 0f32);
            for p in line[starts[c] * 4..starts[c + 1] * 4].chunks_exact(4) {
                if p[3] == 0 {
                    continue;
                }
                let pa = p[3] as f32 * (1.0 / 255.0);
                r += lin[p[0] as usize] * pa;
                g += lin[p[1] as usize] * pa;
                b += lin[p[2] as usize] * pa;
                a += pa;
            }
            s[0] += r;
            s[1] += g;
            s[2] += b;
            s[3] += a;
            s[4] += (starts[c + 1] - starts[c]) as f32;
        }
    }
    sum.iter()
        .map(|s| {
            let a = (s[3] / s[4].max(1.0) * ALPHA_LEVELS as f32).round() as i32;
            if s[3] <= 0.0 {
                return Cell { a, c: [0; 3] };
            }
            let [r, g, b] = [0, 1, 2].map(|i| linear_to_srgb(s[i] / s[3]) as f32);
            let q = |v: f32, step: i32, lo: i32, hi: i32| ((v / step as f32).round() as i32).clamp(lo, hi);
            Cell {
                a,
                c: [q((r + 2.0 * g + b) / 4.0, Y_STEP, 0, 64), q(r - b, C_STEP, -32, 32), q((2.0 * g - r - b) / 2.0, C_STEP, -32, 32)],
            }
        })
        .collect()
}

const BINS: usize = 14; // unary magnitude bins before the escape
const N_CTX: usize = 4 * 3 * (2 + BINS);

/// Adaptive binary contexts for one signed residual: zero flag, sign, unary magnitude, then
/// 7 raw bits past `BINS`. `comp` is 0 = alpha, 1..=3 = Y, Co, Cg; `class` is neighbour activity.
fn ctx(comp: usize, class: usize, slot: usize) -> usize {
    (comp * 3 + class) * (2 + BINS) + slot
}

trait Coder {
    fn bit(&mut self, p: &mut u16, b: bool) -> bool;
}

impl Coder for RcEnc {
    fn bit(&mut self, p: &mut u16, b: bool) -> bool {
        RcEnc::bit(self, p, b);
        b
    }
}

impl Coder for RcDec<'_> {
    fn bit(&mut self, p: &mut u16, _: bool) -> bool {
        RcDec::bit(self, p)
    }
}

fn code<C: Coder>(c: &mut C, probs: &mut [u16], comp: usize, class: usize, v: i32) -> i32 {
    if !c.bit(&mut probs[ctx(comp, class, 0)], v != 0) {
        return 0;
    }
    let neg = c.bit(&mut probs[ctx(comp, class, 1)], v < 0);
    let mut m = 1;
    while m <= BINS as i32 && c.bit(&mut probs[ctx(comp, class, 1 + m as usize)], v.abs() > m) {
        m += 1;
    }
    if m > BINS as i32 {
        // Escape: the rest in 7 bits at a fixed 50/50 (rare, so not worth a context).
        let rest = v.abs() - m;
        for i in (0..7).rev() {
            let mut half = 1024u16;
            m += (c.bit(&mut half, rest >> i & 1 == 1) as i32) << i;
        }
    }
    if neg { -m } else { m }
}

/// Median edge detector (as in JPEG-LS) over the available neighbours.
fn predict(l: Option<i32>, u: Option<i32>, ul: Option<i32>, fallback: i32) -> i32 {
    match (l, u, ul) {
        (Some(l), Some(u), Some(ul)) => {
            if ul >= l.max(u) {
                l.min(u)
            } else if ul <= l.min(u) {
                l.max(u)
            } else {
                l + u - ul
            }
        }
        (Some(l), Some(u), None) => (l + u) / 2,
        (Some(v), None, _) | (None, Some(v), _) => v,
        _ => fallback,
    }
}

fn activity(l: Option<i32>, u: Option<i32>, ul: Option<i32>) -> usize {
    let d = match (l, u, ul) {
        (Some(l), Some(u), Some(ul)) => (l - ul).abs() + (u - ul).abs(),
        (Some(l), Some(u), None) => (l - u).abs(),
        _ => return 2,
    };
    match d {
        0 => 0,
        1..=2 => 1,
        _ => 2,
    }
}

/// Runs the cell coder in either direction; `cells` is read when encoding, filled when decoding.
fn code_cells<C: Coder>(c: &mut C, cells: &mut [Cell], cols: usize, has_alpha: bool) {
    let mut probs = vec![1024u16; N_CTX];
    let mut last = [32, 0, 0];
    for i in 0..cells.len() {
        let (x, y) = (i % cols, i / cols);
        let at = |dx: usize, dy: usize| (x >= dx && y >= dy).then(|| cells[(y - dy) * cols + x - dx]);
        let (l, u, ul) = (at(1, 0), at(0, 1), at(1, 1));
        if has_alpha {
            let get = |n: Option<Cell>| n.map(|n| n.a);
            let p = predict(get(l), get(u), get(ul), ALPHA_LEVELS);
            let a = p + code(c, &mut probs, 0, activity(get(l), get(u), get(ul)), cells[i].a - p);
            cells[i].a = a.clamp(0, ALPHA_LEVELS);
        } else {
            cells[i].a = ALPHA_LEVELS;
        }
        if cells[i].a == 0 {
            continue;
        }
        for (k, last) in last.iter_mut().enumerate() {
            // Only visible neighbours predict colour: transparent cells carry none.
            let get = |n: Option<Cell>| n.filter(|n| n.a > 0).map(|n| n.c[k]);
            let p = predict(get(l), get(u), get(ul), *last);
            let v = p + code(c, &mut probs, k + 1, activity(get(l), get(u), get(ul)), cells[i].c[k] - p);
            cells[i].c[k] = v;
            *last = v;
        }
    }
}

/// Encodes straight sRGB RGBA8 into a URL-safe hash with `detail` cells on the long side
/// (clamped to [`DETAIL_RANGE`]; [`DETAIL`](crate::DETAIL) is the default).
pub fn encode(width: usize, height: usize, rgba: &[u8], detail: usize) -> String {
    assert!(width > 0 && height > 0 && rgba.len() == width * height * 4, "rgba must be width * height * 4 bytes");
    let detail = detail.clamp(*DETAIL_RANGE.start(), *DETAIL_RANGE.end());
    let (cols, rows) = grid_for(width, height, detail);
    let mut cells = cells_of(width, height, rgba, cols, rows);
    let has_alpha = cells.iter().any(|c| c.a < ALPHA_LEVELS);
    let header = (VERSION as u16) << 13 | (has_alpha as u16) << 12 | ((cols - 1) as u16) << 7 | ((rows - 1) as u16) << 2;
    let mut rc = RcEnc::new();
    code_cells(&mut rc, &mut cells, cols, has_alpha);
    let mut bytes = header.to_be_bytes().to_vec();
    bytes.extend(rc.finish());
    base64url(&bytes)
}

fn header(bytes: &[u8]) -> Result<(bool, usize, usize), Error> {
    let h = u16::from_be_bytes([*bytes.first().ok_or(Error::InvalidHash)?, *bytes.get(1).ok_or(Error::InvalidHash)?]);
    if (h >> 13) as u8 != VERSION {
        return Err(Error::InvalidHash);
    }
    Ok((h >> 12 & 1 == 1, (h >> 7 & 31) as usize + 1, (h >> 2 & 31) as usize + 1))
}

/// The hash's grid as (columns, rows); its ratio is the image's aspect ratio.
pub fn grid(hash: &str) -> Result<(usize, usize), Error> {
    let (_, cols, rows) = header(&from_base64url(hash)?)?;
    Ok((cols, rows))
}

/// The cells of a hash, row-major.
pub(crate) fn cells(hash: &str) -> Result<(usize, usize, Vec<Cell>), Error> {
    let bytes = from_base64url(hash)?;
    let (has_alpha, cols, rows) = header(&bytes)?;
    let mut cells = vec![Cell::default(); cols * rows];
    code_cells(&mut RcDec::new(&bytes[2..]), &mut cells, cols, has_alpha);
    Ok((cols, rows, cells))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A disc with a colour gradient on transparency.
    pub(crate) fn disc(w: usize, h: usize) -> Vec<u8> {
        (0..w * h)
            .flat_map(|i| {
                let (x, y) = ((i % w) as i32 - w as i32 / 2, (i / w) as i32 - h as i32 / 2);
                let inside = x * x + y * y < (w.min(h) as i32 / 3).pow(2);
                [(i % w * 255 / w) as u8, 90, 200, if inside { 255 } else { 0 }]
            })
            .collect()
    }

    #[test]
    fn cells_round_trip_exactly() {
        for (w, h, detail) in [(120, 90, 16), (90, 160, 24), (7, 3, 32), (1, 1, 2), (300, 40, 9)] {
            let px = disc(w, h);
            let hash = encode(w, h, &px, detail);
            let (cols, rows, got) = cells(&hash).unwrap();
            assert_eq!((cols, rows), grid_for(w, h, detail.clamp(2, 32)));
            let mut want = cells_of(w, h, &px, cols, rows);
            // Colour of fully transparent cells is not stored.
            want.iter_mut().filter(|c| c.a == 0).for_each(|c| c.c = [0; 3]);
            let mut got = got;
            got.iter_mut().filter(|c| c.a == 0).for_each(|c| c.c = [0; 3]);
            assert_eq!(got, want, "{w}x{h} detail {detail}");
        }
    }

    #[test]
    fn opaque_has_no_alpha_and_hash_is_url_safe() {
        let px: Vec<u8> = (0..64 * 48).flat_map(|i| [(i % 64 * 4) as u8, (i / 64 * 5) as u8, 128, 255]).collect();
        let hash = encode(64, 48, &px, 16);
        assert!(hash.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'), "{hash}");
        assert!(!header(&from_base64url(&hash).unwrap()).unwrap().0);
        assert_eq!(grid(&hash).unwrap(), (16, 12));
    }

    #[test]
    fn bad_hashes_are_rejected() {
        assert_eq!(grid(""), Err(Error::InvalidHash));
        assert_eq!(grid("AAAA"), Err(Error::InvalidHash)); // version 0
        assert_eq!(grid("a/b"), Err(Error::InvalidHash));
    }
}
