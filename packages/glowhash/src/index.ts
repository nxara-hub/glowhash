// A port of the Rust decoder (crates/glowhash/src/{codec,render}.rs) to the format in
// spec/FORMAT.md; spec/vectors.json keeps the two in step.

const B64URL = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_'

/** Blur radius (standard deviation) in cells. */
const SOFTNESS = 0.6
const ALPHA_LEVELS = 15
const BINS = 14

/** A cell: alpha 0..15 and quantised colour (Y, Co, Cg). */
interface Cell {
  a: number
  c: [number, number, number]
}

function bytesOf(hash: string): Uint8Array {
  const out: number[] = []
  let acc = 0
  let bits = 0
  for (const ch of hash) {
    const v = B64URL.indexOf(ch)
    if (v < 0) throw new TypeError('invalid glowhash hash')
    acc = ((acc << 6) | v) & 0xffffff
    bits += 6
    if (bits >= 8) {
      bits -= 8
      out.push((acc >> bits) & 255)
    }
  }
  return Uint8Array.from(out)
}

function header(b: Uint8Array): { alpha: boolean; cols: number; rows: number } {
  if (b.length < 2) throw new TypeError('invalid glowhash hash')
  const h = (b[0] << 8) | b[1]
  if (h >> 13 !== 1) throw new TypeError('invalid glowhash hash')
  return { alpha: ((h >> 12) & 1) === 1, cols: ((h >> 7) & 31) + 1, rows: ((h >> 2) & 31) + 1 }
}

/** LZMA-style binary range decoder, 11-bit probabilities (spec/FORMAT.md §3). */
function rangeDecoder(data: Uint8Array) {
  let pos = 0
  const next = () => data[pos++] ?? 0
  let code = 0
  let range = 0xffffffff
  for (let i = 0; i < 4; i++) code = code * 256 + next()
  return (probs: Uint16Array, i: number): boolean => {
    const bound = (range >>> 11) * probs[i]
    let bit: boolean
    if (code < bound) {
      range = bound
      probs[i] += (2048 - probs[i]) >> 4
      bit = false
    } else {
      code -= bound
      range -= bound
      probs[i] -= probs[i] >> 4
      bit = true
    }
    if (range < 0x1000000) {
      range *= 256
      code = code * 256 + next()
    }
    return bit
  }
}

function predict(l: number | undefined, u: number | undefined, ul: number | undefined, fallback: number): number {
  if (l !== undefined && u !== undefined && ul !== undefined) {
    if (ul >= Math.max(l, u)) return Math.min(l, u)
    if (ul <= Math.min(l, u)) return Math.max(l, u)
    return l + u - ul
  }
  if (l !== undefined && u !== undefined) return Math.trunc((l + u) / 2)
  return l ?? u ?? fallback
}

function activity(l: number | undefined, u: number | undefined, ul: number | undefined): number {
  let d: number
  if (l !== undefined && u !== undefined && ul !== undefined) d = Math.abs(l - ul) + Math.abs(u - ul)
  else if (l !== undefined && u !== undefined) d = Math.abs(l - u)
  else return 2
  return d === 0 ? 0 : d <= 2 ? 1 : 2
}

function cells(hash: string): { cols: number; rows: number; cells: Cell[] } {
  const bytes = bytesOf(hash)
  const { alpha, cols, rows } = header(bytes)
  const bit = rangeDecoder(bytes.subarray(2))
  const probs = new Uint16Array(4 * 3 * (2 + BINS)).fill(1024)
  const half = new Uint16Array(1)
  const value = (comp: number, cls: number): number => {
    const ctx = (slot: number) => (comp * 3 + cls) * (2 + BINS) + slot
    if (!bit(probs, ctx(0))) return 0
    const neg = bit(probs, ctx(1))
    let m = 1
    while (m <= BINS && bit(probs, ctx(1 + m))) m++
    if (m > BINS) {
      for (let i = 6; i >= 0; i--) {
        half[0] = 1024
        if (bit(half, 0)) m += 1 << i
      }
    }
    return neg ? -m : m
  }
  const out: Cell[] = []
  const last = [32, 0, 0]
  for (let i = 0; i < cols * rows; i++) {
    const x = i % cols
    const y = Math.floor(i / cols)
    const l = x > 0 ? out[i - 1] : undefined
    const u = y > 0 ? out[i - cols] : undefined
    const ul = x > 0 && y > 0 ? out[i - cols - 1] : undefined
    const cell: Cell = { a: ALPHA_LEVELS, c: [0, 0, 0] }
    if (alpha) {
      const p = predict(l?.a, u?.a, ul?.a, ALPHA_LEVELS)
      cell.a = Math.min(ALPHA_LEVELS, Math.max(0, p + value(0, activity(l?.a, u?.a, ul?.a))))
    }
    if (cell.a > 0) {
      for (let k = 0; k < 3; k++) {
        // Only visible neighbours predict colour: transparent cells carry none.
        const get = (n: Cell | undefined) => (n && n.a > 0 ? n.c[k] : undefined)
        const p = predict(get(l), get(u), get(ul), last[k])
        cell.c[k] = p + value(k + 1, activity(get(l), get(u), get(ul)))
        last[k] = cell.c[k]
      }
    }
    out.push(cell)
  }
  return { cols, rows, cells: out }
}

/** Straight sRGB RGBA of a cell. */
function rgba(cell: Cell): [number, number, number, number] {
  const [y, co, cg] = [cell.c[0] * 4, cell.c[1] * 8, cell.c[2] * 8]
  const g = y + cg / 2
  const t = y - cg / 2
  const ch = (v: number) => Math.min(255, Math.max(0, Math.round(v)))
  return [ch(t + co / 2), ch(g), ch(t - co / 2), Math.trunc((cell.a * 255) / ALPHA_LEVELS)]
}

const toLinear = (v: number) => {
  const c = v / 255
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
}
const toSrgb = (v: number) => {
  const c = Math.min(1, Math.max(0, Math.round(Math.min(1, Math.max(0, v)) * 4095) / 4095))
  return Math.floor((c <= 0.0031308 ? c * 12.92 : 1.055 * c ** (1 / 2.4) - 0.055) * 255 + 0.5)
}

/** Gaussian taps from `n` cells to `len` samples (spec/FORMAT.md §6). */
function weights(n: number, len: number, extend: boolean): [number, number][][] {
  const sigma = Math.sqrt(SOFTNESS * SOFTNESS + 1 / 6)
  const reach = Math.ceil(3 * sigma)
  const all: [number, number][][] = []
  for (let x = 0; x < len; x++) {
    const u = ((x + 0.5) * n) / len - 0.5
    const taps = new Map<number, number>()
    let total = 0
    for (let i = Math.floor(u) - reach; i <= Math.floor(u) + reach + 1; i++) {
      const w = Math.exp(-((i - u) ** 2) / (2 * sigma * sigma))
      total += w
      if (!extend && (i < 0 || i >= n)) continue
      const k = Math.min(n - 1, Math.max(0, i))
      taps.set(k, (taps.get(k) ?? 0) + w)
    }
    all.push([...taps].map(([k, w]) => [k, w / total]))
  }
  return all
}

/** Straight sRGB RGBA pixels, shaped like `ImageData`. */
export interface Pixels {
  data: Uint8ClampedArray
  width: number
  height: number
}

/**
 * Renders the hash as straight sRGB RGBA pixels. With no size, 32 px on the long side; with one,
 * the other follows the image's aspect ratio. Small sizes scaled up by the browser look the same
 * and decode fastest.
 */
export function decode(hash: string, width?: number, height?: number): Pixels {
  const { cols, rows, cells: cs } = cells(hash)
  const follow = (v: number, a: number, b: number) => Math.max(1, Math.round((v * a) / b))
  if (width === undefined && height === undefined) {
    if (cols >= rows) width = 32
    else height = 32
  }
  width ??= follow(height!, cols, rows)
  height ??= follow(width, rows, cols)
  if (!(width >= 1 && height >= 1)) throw new RangeError('width and height must be at least 1')
  const px = cs.map(rgba)
  const opaque = px.every((p) => p[3] === 255)
  const prem = px.map((p) => {
    const a = p[3] / 255
    return [toLinear(p[0]) * a, toLinear(p[1]) * a, toLinear(p[2]) * a, a]
  })
  const wx = weights(cols, width, opaque)
  const wy = weights(rows, height, opaque)
  const mid = new Float64Array(rows * width * 4)
  for (let r = 0; r < rows; r++)
    for (let x = 0; x < width; x++)
      for (const [i, t] of wx[x]) for (let k = 0; k < 4; k++) mid[(r * width + x) * 4 + k] += prem[r * cols + i][k] * t
  const out = new Uint8ClampedArray(width * height * 4)
  for (let y = 0; y < height; y++)
    for (let x = 0; x < width; x++) {
      const p = [0, 0, 0, 0]
      for (const [r, t] of wy[y]) for (let k = 0; k < 4; k++) p[k] += mid[(r * width + x) * 4 + k] * t
      const a = Math.min(1, Math.max(0, p[3]))
      const o = (y * width + x) * 4
      for (let k = 0; k < 3; k++) out[o + k] = a > 0 ? toSrgb(p[k] / a) : 0
      out[o + 3] = Math.floor(a * 255 + 0.5)
    }
  return { data: out, width, height }
}
