//! Hashes every image in a folder and its subfolders in parallel and prints JSON lines:
//! {"file", "hash"}. JPEGs go through `encode_jpeg`, which decodes them at reduced size;
//! `--full` decodes them at full size instead, for comparison.
//!
//!   cargo run --release --features jpeg --example bulk -- <folder> [detail] [--full] > hashes.jsonl

use rayon::prelude::*;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let full = args.iter().any(|a| a == "--full");
    let mut args = args.iter().filter(|a| *a != "--full");
    let dir = args.next().expect("usage: bulk <folder> [detail] [--full]");
    let detail = args.next().map_or(glowhash::DETAIL, |d| d.parse().expect("detail must be a number"));
    let mut files = vec![];
    let mut dirs = vec![std::path::PathBuf::from(dir)];
    while let Some(d) = dirs.pop() {
        for p in std::fs::read_dir(d).unwrap().map(|e| e.unwrap().path()) {
            if p.is_dir() {
                dirs.push(p);
            } else if !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
                files.push(p);
            }
        }
    }
    files.sort();

    let start = Instant::now();
    let rows: Vec<(String, Option<String>, f64)> = files
        .par_iter()
        .map(|p| {
            let name = p.display().to_string();
            let t = Instant::now();
            let jpeg = !full && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"));
            let hash = (|| {
                if jpeg {
                    std::fs::read(p).ok().and_then(|bytes| glowhash::encode_jpeg(&bytes, detail).ok())
                } else {
                    // No size limits: the folder is trusted, and print-master TIFFs can be 200 MB.
                    let mut reader = image::ImageReader::open(p).ok()?.with_guessed_format().ok()?;
                    reader.no_limits();
                    reader.decode().ok().map(|img| {
                        let img = img.to_rgba8();
                        glowhash::encode(img.width() as usize, img.height() as usize, img.as_raw(), detail)
                    })
                }
            })();
            (name, hash, t.elapsed().as_secs_f64())
        })
        .collect();
    let wall = start.elapsed().as_secs_f64();

    let (mut n, mut enc, mut chars) = (0, 0.0, 0);
    for (file, hash, secs) in &rows {
        match hash {
            Some(h) => {
                println!("{{\"file\":{file:?},\"hash\":{h:?}}}");
                (n, enc, chars) = (n + 1, enc + secs, chars + h.len());
            }
            None => eprintln!("skipped (not an image): {file}"),
        }
    }
    if n > 0 {
        eprintln!(
            "{n} images in {wall:.2} s; {:.2} ms per image per core, file decoding included; hashes average {} chars",
            enc * 1e3 / n as f64,
            chars / n
        );
    }
}
