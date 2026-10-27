# glowhash

Blurred image placeholders that keep the subject's colours, with real transparency: a short URL-safe hash, decoded to pixels at any size.

```rust
let hash = glowhash::encode(width, height, &rgba, glowhash::DETAIL); // straight sRGB RGBA8 in
let (w, h) = glowhash::size_for(&hash, 32)?;
let pixels = glowhash::decode(&hash, w, h)?;      // straight sRGB RGBA8 out
```

For an `<img src>`, save the decoded pixels as a PNG and use it as a data URL; `examples/placeholder.rs` shows how.

`detail` is the number of cells on the image's long side (2–32, default 16): more cells keep more of the shape and colours, and make a longer hash. Encoding is one pass over the pixels, about 1.2 ms per megapixel on one core, with no shared state, so bulk jobs parallelise freely (see `examples/bulk.rs`).

**JPEG files in bulk.** With the `jpeg` feature, `glowhash::encode_jpeg(&bytes, detail)` hashes a JPEG while decoding it at 1/2, 1/4 or 1/8 size, which is plenty for the cells. For files of 24–48 MP that is about 4.5–6× faster end to end and uses up to 64× less memory; for 12 MP and smaller it is 1.0–1.5× faster, because unpacking the compressed data, which can't be skipped, dominates. Its hashes are not byte-identical to hashing a full decode, but render within a few levels (a median of under 1 level out of 255 on real photos).

In JavaScript, `@glowhash/node` runs this crate natively in Node, and the `glowhash` npm package decodes the same hashes in the browser.
