import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { test } from 'node:test'
import { deflateSync } from 'node:zlib'

const { encode, decode } = createRequire(import.meta.url)('../index.js')
const vectors = JSON.parse(readFileSync(new URL('../../../spec/vectors.json', import.meta.url)))

// The synthetic images behind the vectors, as made in crates/glowhash/tests/vectors.rs (in f32).
const CASES = [[0, 64, 48], [1, 60, 80], [2, 90, 40], [3, 33, 77]]
const DETAILS = [4, 9, 16, 32]
function image(kind, w, h) {
  const f = Math.fround
  const px = new Uint8Array(w * h * 4)
  for (let i = 0; i < w * h; i++) {
    const [x, y] = [i % w, Math.floor(i / w)]
    const [fx, fy] = [f(x / w), f(y / h)]
    const d = f(f(f(fx - 0.5) ** 2) + f(f(fy - 0.5) ** 2))
    px.set(
      kind === 0 ? [f(fx * 255), f(fy * 255), 128, 255]
      : kind === 1 ? [220, 40, 50, d < f(0.1) ? 255 : 0]
      : kind === 2 ? [x % 9 < 4 ? 250 : 20, f(fy * 200), 60, f(fx * 255)]
      : (Math.floor(x / 7) + Math.floor(y / 5)) % 2 === 0 ? [30, 90, 200, 255] : [240, 200, 20, 255],
      i * 4,
    )
  }
  return px
}

// A minimal RGBA PNG file, to hash as a file.
function png(w, h, rgba) {
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    for (let k = 0; k < 8; k++) n = n & 1 ? 0xedb88320 ^ (n >>> 1) : n >>> 1
    return n >>> 0
  })
  const chunk = (kind, data) => {
    const body = Buffer.concat([Buffer.from(kind), data])
    let c = 0xffffffff
    for (const v of body) c = crcTable[(c ^ v) & 255] ^ (c >>> 8)
    const out = Buffer.alloc(body.length + 8)
    out.writeUInt32BE(data.length, 0)
    body.copy(out, 4)
    out.writeUInt32BE((c ^ 0xffffffff) >>> 0, body.length + 4)
    return out
  }
  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(w, 0)
  ihdr.writeUInt32BE(h, 4)
  ihdr.set([8, 6, 0, 0, 0], 8)
  const raw = Buffer.alloc(h * (1 + w * 4))
  for (let y = 0; y < h; y++) raw.set(rgba.subarray(y * w * 4, (y + 1) * w * 4), y * (1 + w * 4) + 1)
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))])
}

test('encode pixels gives the vector hashes', async () => {
  const hashes = await Promise.all(CASES.flatMap(([kind, w, h]) => DETAILS.map((d) => encode({ data: Buffer.from(image(kind, w, h)), width: w, height: h }, d))))
  assert.deepEqual(hashes, vectors.map((v) => v.hash))
})

test('encode a PNG file gives the same hash as its pixels', async () => {
  const px = image(2, 90, 40)
  assert.equal(await encode(png(90, 40, px), 16), await encode({ data: Buffer.from(px), width: 90, height: 40 }, 16))
})

test('decode gives the vector pixels', () => {
  for (const v of vectors) {
    const out = decode(v.hash, v.width, v.height)
    assert.deepEqual([out.width, out.height], [v.width, v.height])
    assert.deepEqual([...out.data], v.pixels)
  }
  const [cols, rows] = vectors[1].grid
  const { width, height } = decode(vectors[1].hash)
  assert.equal(Math.max(width, height), 32)
  assert.equal(decode(vectors[1].hash, 60).height, Math.round((60 * rows) / cols))
})

test('bad input is rejected', async () => {
  await assert.rejects(encode(Buffer.from('not an image')))
  await assert.rejects(encode({ data: Buffer.alloc(15), width: 2, height: 2 }))
  assert.throws(() => decode('a+b'))
  assert.throws(() => decode('AAAA'))
})
