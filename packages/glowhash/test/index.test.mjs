import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { decode } from '../dist/index.js'

// The frozen vectors from spec/vectors.json: Rust-made hashes with the Rust decode of each.
const vectors = JSON.parse(readFileSync(new URL('../../../spec/vectors.json', import.meta.url)))

test('decode matches the Rust decoder to within one level', () => {
  for (const v of vectors) {
    const { data, width, height } = decode(v.hash, v.width, v.height)
    assert.deepEqual([width, height, data.length], [v.width, v.height, v.pixels.length])
    let worst = 0
    for (let i = 0; i < data.length; i++) {
      // Colour under full transparency is invisible; skip it.
      if (i % 4 !== 3 && v.pixels[i - (i % 4) + 3] === 0) continue
      worst = Math.max(worst, Math.abs(data[i] - v.pixels[i]))
    }
    assert.ok(worst <= 1, `${v.hash.slice(0, 12)}…: off by ${worst}`)
  }
})

test('decode sizes follow the aspect ratio', () => {
  for (const v of vectors) {
    const [cols, rows] = v.grid
    const { width, height } = decode(v.hash)
    assert.equal(Math.max(width, height), 32)
    assert.deepEqual(decode(v.hash, 24).height, Math.max(1, Math.round((24 * rows) / cols)))
    // The vectors were rendered at 24 px on the long side.
    assert.deepEqual(cols >= rows ? [24, decode(v.hash, 24).height] : [decode(v.hash, undefined, 24).width, 24], [v.width, v.height])
  }
})

test('bad hashes throw', () => {
  assert.throws(() => decode(''), TypeError)
  assert.throws(() => decode('AAAA'), TypeError)
  assert.throws(() => decode('a+b'), TypeError)
  assert.throws(() => decode(vectors[0].hash, 0, 4), RangeError)
})
