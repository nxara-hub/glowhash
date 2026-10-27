//! Bulk speed at different image sizes: glowhash encode alone (one core and all cores), and
//! the full per-file cost including decoding a JPEG (in full, and scaled with `encode_jpeg`)
//! or a PNG first.
//!
//!   cargo run --release --features jpeg --example speed -- photo.png

use rayon::prelude::*;
use std::time::Instant;

const SIZES: [(u32, u32); 6] = [(600, 450), (1200, 900), (2400, 1800), (4000, 3000), (6000, 4000), (8000, 6000)];

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: speed <photo>");
    let src = image::open(&path).unwrap().to_rgba8();
    let cores = rayon::current_num_threads();
    println!("cores: {cores}\n");
    println!(
        "| image | megapixels | encode, 1 core | encode, all cores | JPEG full decode + encode | JPEG encode_jpeg (scaled) | PNG decode + encode |"
    );
    println!("|---|---|---|---|---|---|---|");
    for (w, h) in SIZES {
        // A photo scaled to this size, so the pixels look like a real image.
        let img = image::imageops::resize(&src, w, h, image::imageops::FilterType::Triangle);
        let (wu, hu) = (w as usize, h as usize);
        let mp = (w * h) as f64 / 1e6;

        let reps = (40.0 / mp).ceil().clamp(3.0, 40.0) as usize;
        let one = median(
            (0..reps)
                .map(|_| {
                    let t = Instant::now();
                    std::hint::black_box(glowhash::encode(wu, hu, img.as_raw(), glowhash::DETAIL));
                    t.elapsed().as_secs_f64()
                })
                .collect(),
        );

        // Bulk: many images at once across all cores.
        let n = (cores * 4).max(16);
        let t = Instant::now();
        (0..n).into_par_iter().for_each(|_| {
            std::hint::black_box(glowhash::encode(wu, hu, img.as_raw(), glowhash::DETAIL));
        });
        let par = n as f64 / t.elapsed().as_secs_f64();

        // Full per-file cost (files per second, all cores): decode the file, then hash it.
        let file = |fmt: image::ImageFormat, scaled: bool| -> f64 {
            let mut bytes = vec![];
            image::DynamicImage::ImageRgba8(img.clone()).to_rgb8().write_to(&mut std::io::Cursor::new(&mut bytes), fmt).unwrap();
            let n = (cores * if scaled { 8 } else { 2 }).max(8);
            let t = Instant::now();
            (0..n).into_par_iter().for_each(|_| {
                if scaled {
                    std::hint::black_box(glowhash::encode_jpeg(&bytes, glowhash::DETAIL).unwrap());
                } else {
                    let px = image::load_from_memory(&bytes).unwrap().to_rgba8();
                    std::hint::black_box(glowhash::encode(px.width() as usize, px.height() as usize, px.as_raw(), glowhash::DETAIL));
                }
            });
            n as f64 / t.elapsed().as_secs_f64()
        };
        let (jpg, fast, png) =
            (file(image::ImageFormat::Jpeg, false), file(image::ImageFormat::Jpeg, true), file(image::ImageFormat::Png, false));

        println!(
            "| {w}×{h} | {mp:.1} | {:.1} ms ({:.0} MP/s) | {par:.0} images/s ({:.0} MP/s) | {jpg:.0} files/s | {fast:.0} files/s ({:.1}×) | {png:.0} files/s |",
            one * 1e3,
            mp / one,
            par * mp,
            fast / jpg
        );
    }
}
