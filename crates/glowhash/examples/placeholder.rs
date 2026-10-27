//! One image: prints its hash, then turns the hash into an `<img src>` (a PNG data URL) and
//! writes an HTML page showing it next to the original.
//!
//!   cargo run --release --example placeholder -- photo.png [detail]

use std::io::Cursor;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: placeholder <image> [detail]");
    let detail = args.next().map_or(glowhash::DETAIL, |d| d.parse().expect("detail must be a number"));
    let img = image::open(&path).unwrap_or_else(|e| panic!("can't read {path}: {e}")).to_rgba8();

    let hash = glowhash::encode(img.width() as usize, img.height() as usize, img.as_raw(), detail);
    println!("{hash}");

    // Hash -> img src: decode small (the browser scales it up smoothly), save as PNG, base64 it.
    let (w, h) = glowhash::size_for(&hash, 32).unwrap();
    let pixels = glowhash::decode(&hash, w, h).unwrap();
    let mut png = vec![];
    image::RgbaImage::from_raw(w as u32, h as u32, pixels).unwrap().write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let src = format!("data:image/png;base64,{}", base64(&png));

    let html = format!("{path}.placeholder.html");
    let name = std::path::Path::new(&path).file_name().unwrap().to_string_lossy();
    let style = "height:80vh";
    std::fs::write(&html, format!("<!doctype html><meta charset=utf-8><body style='margin:0;display:flex;gap:16px;padding:16px;background:#1b1e26'><img src='{name}' style='{style}'><img src='{src}' style='{style}'>")).unwrap();
    eprintln!("{} chars; img src {} chars -> {html}", hash.len(), src.len());
}

fn base64(bytes: &[u8]) -> String {
    const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            s.push(if i <= c.len() { B64[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    s
}
