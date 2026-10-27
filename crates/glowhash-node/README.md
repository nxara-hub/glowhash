# @glowhash/node

glowhash for Node.js: the Rust encoder and decoder as a native module, with a prebuilt binary for macOS, Linux and Windows (x64 and arm64). glowhash placeholders are blurred previews that keep the subject's colours, with real transparency.

```js
import { encode, decode } from '@glowhash/node'

const hash = await encode(await fs.readFile('photo.jpg'))          // image file bytes
const same = await encode({ data: rgba, width, height }, 16)      // or raw RGBA pixels
const { data, width, height } = decode(hash)                       // RGBA, 32 px on the long side
```

- `encode(input, detail = 16)`: a hash from image file bytes (JPEG, PNG, WebP or TIFF) or straight sRGB RGBA pixels `{ data, width, height }`, with `detail` cells (2–32) on the long side. Runs on libuv's thread pool, so `Promise.all` over many images uses several cores (`UV_THREADPOOL_SIZE`, default 4). JPEGs are decoded at reduced size (up to 1/8), which is most of the work on big photos.
- `decode(hash, width?, height?)`: straight sRGB RGBA pixels `{ data, width, height }`. With no size, 32 px on the long side; with one, the other follows the image's aspect ratio.

Speed on an M1 Pro, JPEG files in: 4.5 ms for 1.4 MP, 18 ms for 9 MP and 113 ms for 36 MP per file on one thread; with `Promise.all` on 4 threads, 840, 216 and 34 files per second.

## Hash to `<img src>`

Decode small, save as PNG, and use it as a data URL; the browser scales it up smoothly. With [sharp](https://sharp.pixelplumbing.com):

```js
import sharp from 'sharp'
import { decode } from '@glowhash/node'

const { data, width, height } = decode(hash)
const png = await sharp(data, { raw: { width, height, channels: 4 } }).png().toBuffer()
const src = `data:image/png;base64,${png.toString('base64')}` // about 1–3 KB
```

In the browser, the `glowhash` package decodes the same hashes.
