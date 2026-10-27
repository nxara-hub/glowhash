//! The frozen test vectors in spec/vectors.json: synthetic images hashed at several detail
//! levels, with the decode of each at a small size. This test re-encodes the images and checks
//! both the hash and the decode, so any change to the format shows up here.
//!
//!   cargo test -p glowhash --test vectors                       # check
//!   UPDATE_VECTORS=1 cargo test -p glowhash --test vectors      # rewrite after a deliberate change

const CASES: [(usize, (usize, usize)); 4] = [(0, (64, 48)), (1, (60, 80)), (2, (90, 40)), (3, (33, 77))];
const DETAILS: [usize; 4] = [4, 9, 16, 32];

fn image(kind: usize, w: usize, h: usize) -> Vec<u8> {
    (0..w * h)
        .flat_map(|i| {
            let (x, y) = (i % w, i / w);
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            match kind {
                // opaque gradient
                0 => [(fx * 255.0) as u8, (fy * 255.0) as u8, 128, 255],
                // red disc on transparency
                1 => {
                    let d = (fx - 0.5).powi(2) + (fy - 0.5).powi(2);
                    [220, 40, 50, if d < 0.1 { 255 } else { 0 }]
                }
                // alpha ramp with colour stripes, touching the frame
                2 => [if x % 9 < 4 { 250 } else { 20 }, (fy * 200.0) as u8, 60, (fx * 255.0) as u8],
                // two-colour checker, opaque
                _ => {
                    if (x / 7 + y / 5) % 2 == 0 {
                        [30, 90, 200, 255]
                    } else {
                        [240, 200, 20, 255]
                    }
                }
            }
        })
        .collect()
}

struct Vector {
    hash: String,
    grid: (usize, usize),
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

fn generate() -> Vec<Vector> {
    let mut out = vec![];
    for (kind, (w, h)) in CASES {
        for detail in DETAILS {
            let hash = glowhash::encode(w, h, &image(kind, w, h), detail);
            let (width, height) = glowhash::size_for(&hash, 24).unwrap();
            let pixels = glowhash::decode(&hash, width, height).unwrap();
            out.push(Vector { grid: glowhash::grid(&hash).unwrap(), hash, width, height, pixels });
        }
    }
    out
}

fn json(vs: &[Vector]) -> String {
    let rows: Vec<String> = vs
        .iter()
        .map(|v| {
            let px = v.pixels.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",");
            format!(
                "{{\"hash\":\"{}\",\"grid\":[{},{}],\"width\":{},\"height\":{},\"pixels\":[{px}]}}",
                v.hash, v.grid.0, v.grid.1, v.width, v.height
            )
        })
        .collect();
    format!("[\n{}\n]\n", rows.join(",\n"))
}

#[test]
fn vectors_are_frozen() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/vectors.json");
    let want = json(&generate());
    if std::env::var_os("UPDATE_VECTORS").is_some() {
        std::fs::write(&path, &want).unwrap();
        return;
    }
    let have = std::fs::read_to_string(&path).expect("spec/vectors.json missing: run with UPDATE_VECTORS=1");
    for (i, (a, b)) in have.lines().zip(want.lines()).enumerate() {
        assert_eq!(a, b, "vector line {i} changed; if the format change is deliberate, run with UPDATE_VECTORS=1");
    }
    assert_eq!(have.lines().count(), want.lines().count());
}
