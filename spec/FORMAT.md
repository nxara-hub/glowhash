# glowhash format, version 1

A glowhash hash describes a small grid of colour cells (columns × rows, each with an opacity), written as URL-safe text. A decoder recovers the cells exactly, then renders them as a soft blur. This document is normative for decoders; the encoder section describes the reference encoder.

`spec/vectors.json` holds test vectors (see [Conformance](#conformance)).

## 1. Text

The hash is base64url (RFC 4648 §5, alphabet `A–Z a–z 0–9 - _`) without padding. Decode it to bytes, discarding trailing bits that do not complete a byte. Any other character makes the hash invalid.

## 2. Header

The first two bytes, read as a big-endian 16-bit value `h`:

| bits | field |
|---|---|
| 15–13 | version, must be `1` |
| 12 | `has_alpha` |
| 11–7 | `cols − 1` (1–32 columns) |
| 6–2 | `rows − 1` (1–32 rows) |
| 1–0 | reserved, written as 0, ignored |

A hash shorter than two bytes, or with another version, is invalid. `cols : rows` approximates the image's aspect ratio.

## 3. Range decoder

Everything after the header is one range-coded stream. The decoder state is two unsigned 32-bit integers, `code` and `range`.

- **Start:** `range = 0xFFFFFFFF`; `code` = the first four stream bytes, big-endian. Reading past the end of the stream yields `0`.
- **Probabilities:** 11-bit values (out of 2048), each starting at `1024`.
- **Decoding one bit with probability `p`:**
  1. `bound = (range >> 11) * p`
  2. If `code < bound`: the bit is 0, `range = bound`, `p += (2048 − p) >> 4`.
  3. Otherwise: the bit is 1, `code −= bound`, `range −= bound`, `p −= p >> 4`.
  4. If `range < 2^24`: `range <<= 8`, `code = (code << 8) | next byte`.

## 4. Cells

Cells are decoded in raster order (row by row, left to right). Each cell has an alpha `a` in 0–15 and three colour values `Y, Co, Cg` (integers; quantised units, see §5).

### 4.1 Residuals

A signed residual is read with three groups of adaptive probabilities. They are selected by a component `comp` (0 = alpha, 1 = Y, 2 = Co, 3 = Cg) and an activity class `class` (0–2, §4.3). Each (comp, class) pair owns 16 probabilities, `P[0..15]`, so there are 4 × 3 × 16 = 192 in total. All start at 1024 and adapt for the whole hash.

1. Read a bit with `P[0]`. If 0, the residual is 0.
2. Read the sign with `P[1]` (1 = negative).
3. Magnitude: `m = 1`; while `m ≤ 14` and a bit read with `P[1 + m]` is 1, increment `m`.
4. Escape: if `m = 15`, read 7 more bits, most significant first, each with a fresh probability of 1024 (not kept). Add their value to `m`.
5. The residual is `±m`.

### 4.2 Prediction

For cell `(x, y)`, let `L`, `U` and `UL` be the already-decoded cells at `(x−1, y)`, `(x, y−1)` and `(x−1, y−1)`, where they exist. For a value `v` (alpha, or one colour component), with neighbour values `l`, `u`, `ul` (each possibly absent):

- `l`, `u` and `ul` all present: the median edge detector. If `ul ≥ max(l, u)`, predict `min(l, u)`; if `ul ≤ min(l, u)`, predict `max(l, u)`; otherwise predict `l + u − ul`.
- `l` and `u` present, `ul` absent: `(l + u) / 2`, truncated toward zero.
- Exactly one of `l`, `u` present: that value (ignoring `ul`).
- Otherwise: the fallback.

### 4.3 Activity class

With the same neighbour values:

- `l`, `u`, `ul` present: `d = |l − ul| + |u − ul|`.
- `l`, `u` present, `ul` absent: `d = |l − u|`.
- Otherwise: class 2.

Class 0 if `d = 0`, class 1 if `d ≤ 2`, class 2 otherwise.

### 4.4 Decoding one cell

Keep `last = [32, 0, 0]` across the whole hash.

1. **Alpha.** If `has_alpha`: the neighbour values are the neighbours' alpha. Predict with fallback `15`, read a residual with comp 0, and set `a = clamp(prediction + residual, 0, 15)`. Without alpha, `a = 15`.
2. If `a = 0`, the cell has no colour: stop.
3. **Colour.** For `k` = 0, 1, 2 (Y, Co, Cg): the neighbour values are the neighbours' colour component `k`, counting only neighbours with `a > 0` (a transparent neighbour is absent). Predict with fallback `last[k]`, read a residual with comp `k + 1`, and set the component to `prediction + residual`. Then set `last[k]` to it.

## 5. Cell colour

A cell with alpha `a` and colour `Y, Co, Cg` is straight (not premultiplied) sRGB:

```
y = 4·Y,  co = 8·Co,  cg = 8·Cg
g = y + cg/2,   t = y − cg/2
R = t + co/2,   G = g,   B = t − co/2      (each clamped to 0–255)
A = 17·a                                   (0–255)
```

All of these are integers. A cell with `a = 0` is fully transparent and its colour is irrelevant.

An image is **opaque** when every cell has `a = 15`.

## 6. Rendering

Rendering blurs the cell grid with a Gaussian. It works in linear light over premultiplied colour, so transparent cells add no colour.

- **Linear light:** sRGB to linear is the standard transfer function (`c ≤ 0.04045 ? c/12.92 : ((c+0.055)/1.055)^2.4`). Premultiply by `A/255`.
- **Kernel:** standard deviation `σ = sqrt(0.6² + 1/6)` cells, where `0.6` is the blur (`SOFTNESS`) and `1/6` stands in for the bilinear upscale that usually shows a small decode at full size. Separable: rows first, then columns.
- **Sample positions:** for an output of `n` samples along a grid of `N` cells, sample `x` sits at `u = (x + 0.5)·N/n − 0.5` in cell coordinates. It takes cells `i` from `⌊u⌋ − R` to `⌊u⌋ + R + 1`, where `R = ⌈3σ⌉`, with weight `exp(−(i − u)²/(2σ²))`. Weights are normalised by their sum over all those `i`.
- **Past the edge:** if the image is opaque, a cell index outside the grid uses the nearest edge cell. Otherwise it is transparent: its weight counts toward the sum but contributes nothing, so the result fades there.
- **Output:** divide colour by alpha (where alpha > 0) and convert back to sRGB. Alpha is `round(255·a)`.

## 7. Encoder (reference)

Other encoders may choose cells differently; any hash that follows §2–§4 is valid.

1. **Grid:** `detail` (2–32) cells on the long side. The short side has `clamp(round(detail · short / long), 1, detail)`.
2. **Cell membership:** pixel `(x, y)` belongs to cell `(⌊x·cols/w⌋, ⌊y·rows/h⌋)`.
3. **Cell values:**
   - alpha = `round(15 · mean pixel alpha)`;
   - colour = `Σ(linear colour × alpha) / Σ alpha`, converted to sRGB 0–255;
   - quantised as `Y = clamp(round((R + 2G + B)/4 / 4), 0, 64)`, `Co = clamp(round((R − B)/8), −32, 32)` and `Cg = clamp(round((2G − R − B)/2 / 8), −32, 32)`.

   Cells with no visible pixels have colour 0.
4. **Header:** `has_alpha` is set when any cell has alpha below 15.
5. **Coding:** cells are coded with the inverse of §4, using an LZMA-style range encoder with the same probability updates.
6. **Flush:** round `low` up to a multiple of 2^16 (propagating any carry) and write its top two bytes.

## Conformance

`spec/vectors.json` lists hashes made by the reference encoder from synthetic images. Each entry has the hash, its grid, and its rendering per §6 at a small size (`width`, `height`, RGBA `pixels`).

- **Decoders** must reproduce the cells exactly; check this through `grid` and the rendering.
- **Renderings** must match within one level per channel, ignoring colour where alpha is 0. Floating-point differences between platforms are allowed for.
- **The reference encoder** reproduces each hash exactly from the images described in `crates/glowhash/tests/vectors.rs`.
