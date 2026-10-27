# glowhash

Decoder for glowhash placeholders, for the browser: blurred previews that keep the subject's colours, with real transparency. Plain TypeScript, no dependencies, no WebAssembly. Hashes are made on the server with `@glowhash/node` (Node) or the `glowhash` crate (Rust).

```js
import { decode } from 'glowhash'

const { data, width, height } = decode(hash) // straight RGBA, shaped like ImageData
```

`decode(hash, width?, height?)` renders the hash at any size. With no size, 32 px on the long side; with one, the other follows the image's aspect ratio. Small sizes scaled up by the browser look the same and decode fastest.

## Hash to `<img src>`

Draw the pixels on a canvas and use its data URL:

```js
import { decode } from 'glowhash'

const { data, width, height } = decode(hash)
const canvas = Object.assign(document.createElement('canvas'), { width, height })
canvas.getContext('2d').putImageData(new ImageData(data, width, height), 0, 0)
img.src = canvas.toDataURL()
```

Or draw the canvas itself in place of the image, sized with CSS.
