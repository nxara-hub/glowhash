# glowhash

Blurred image placeholders that keep the subject's colours, with real transparency. An image becomes a short URL-safe hash; the hash decodes to a soft preview at any size.

- **Real colours.** The image is averaged into a small grid of cells in linear light, weighted by opacity, so each cell holds the true colour of its patch.
- **Real transparency, no bleed.** Transparent pixels add nothing to a cell, and the blur works on premultiplied colour, so the glow reaches about one cell past the outline and never smears colour into empty space or around the frame.
- **Any photo.** Opaque images stay solid to their frame, with a BlurHash-soft blur.
- **Detail you choose.** 2–32 cells on the long side, 16 by default. At 16, hashes are typically 120–200 characters for product cut-outs and 250–350 for full photos.
- **Built for bulk.** Encoding is Rust, also in Node: one pass over the pixels at about 800 megapixels per second per core, and large JPEGs are decoded at reduced size (4.5–6× faster end to end at 24–48 MP).

## Packages

| path | package | what |
|---|---|---|
| `crates/glowhash` | `glowhash` (crates.io, not yet published) | encode and decode in Rust |
| `crates/glowhash-node` | `@glowhash/node` on npm | the Rust crate as a native Node module: encode image files or pixels, and decode |
| `packages/glowhash` | `glowhash` on npm | decode in the browser, in plain TypeScript with no dependencies |
| `spec` | | the format and the frozen test vectors every package is tested against |

### Server (Node): encode

```js
import { encode } from '@glowhash/node'

const hash = await encode(await fs.readFile('photo.jpg')) // JPEG, PNG, WebP or TIFF bytes
const hashes = await Promise.all(files.map((f) => encode(f, 16))) // runs on several cores
```

### Browser: decode to an `<img src>`

`decode` returns straight RGBA pixels shaped like `ImageData`, 32 px on the long side by default. Draw them on a canvas and use its data URL; the browser scales the small image up smoothly.

```js
import { decode } from 'glowhash'

const { data, width, height } = decode(hash)
const canvas = Object.assign(document.createElement('canvas'), { width, height })
canvas.getContext('2d').putImageData(new ImageData(data, width, height), 0, 0)
img.src = canvas.toDataURL()
```

To send pages a ready `<img src>` with no script at all, make the PNG data URL on the server instead (see the `@glowhash/node` README).

### Rust

```rust
let hash = glowhash::encode(width, height, &rgba, glowhash::DETAIL); // straight RGBA8 in
let (w, h) = glowhash::size_for(&hash, 32)?;
let pixels = glowhash::decode(&hash, w, h)?; // straight RGBA8 out, at any size
```

## How it compares

Measured against the real image (DSSIM on the worst of 9 page backgrounds) at equal bytes, on transparent product images, icons and photos:

- **Against BlurHash as sites use it** (flattened on white): closer on transparent images at nearly every size.
- **Against BlurHash's best case** (re-flattened for each background) **and ThumbHash** (about 25 bytes): they score better at small sizes. glowhash pulls ahead from about 96–192 bytes, and from the smallest sizes on mostly transparent product shots, where transparent cells are almost free.

The trade is colours and outlines that stay readable, with a glow that doesn't bleed, at a longer hash than BlurHash's 20–30 characters.

## Develop

Needs Rust and Bun.

```bash
bun install
```

```bash
bun run build
```

```bash
bun run test
```

Turborepo runs each package's own tools: Cargo for the crate, napi-rs for the Node module, TypeScript and Node's test runner for the browser package.

The crate has three examples:

```bash
cargo run --release -p glowhash --example placeholder -- photo.png 16
```

```bash
cargo run --release -p glowhash --features jpeg --example bulk -- <folder> 16 > hashes.jsonl
```

```bash
cargo run --release -p glowhash --features jpeg --example speed -- photo.jpg
```

`placeholder` hashes one image, turns the hash into an `<img src>` and writes an HTML page showing both; `bulk` hashes a folder in parallel; `speed` times hashing and file decoding at 0.3–48 megapixels.

The format is defined in [`spec/FORMAT.md`](spec/FORMAT.md) and frozen by `spec/vectors.json`: every package's tests check their hashes and decodes against it. After a deliberate format change, `UPDATE_VECTORS=1 cargo test -p glowhash --test vectors` rewrites the file.

## License

MIT
