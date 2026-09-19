/** Spike C2 server - streams Annex-B H.264 over a WebSocket.
 *
 * Deliberately dumb. The point is not this server; it is finding out whether a
 * browser's WebCodecs VideoDecoder can take our encoder's output over a plain
 * TCP WebSocket with acceptable latency. If it can, the whole WebRTC / ICE /
 * TURN / coturn layer disappears, because MeshAgent already dials outbound and
 * NAT is therefore already solved.
 *
 * Wire format, per binary message:
 *   [0]      frame type   0 = delta, 1 = key
 *   [1..9]   timestamp    microseconds, BigUint64BE (decode order == display
 *                         order here; MF emits no B-frames at baseline/main
 *                         with low latency set)
 *   [9..17]  sent at      server wall clock ms, BigUint64BE
 *   [17..]   access unit  one decodable picture, Annex-B with start codes
 */

import { createServer } from 'node:http'
import { readFileSync } from 'node:fs'
import { extname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { WebSocketServer } from 'ws'
import { codecStringFromSps, toAccessUnits } from './annexb.mjs'

const here = fileURLToPath(new URL('.', import.meta.url))
const file = process.argv[2] ?? 'sample.h264'
const fps = Number(process.env.FPS ?? 30)
const port = Number(process.env.PORT ?? 5190)
/** Artificial per-send delay. Approximates a slow link; see README for why
 *  this is NOT the same as reproducing TCP head-of-line blocking. */
const jitterMs = Number(process.env.JITTER_MS ?? 0)

const buf = readFileSync(file)
const units = toAccessUnits(buf)
const codec = codecStringFromSps(buf)

if (!units.length) {
  console.error(`no access units found in ${file} - is it raw Annex-B, not MP4?`)
  process.exit(1)
}
if (!codec) {
  console.error(`no SPS found in ${file} - cannot build a codec string`)
  process.exit(1)
}

console.log(`${file}: ${units.length} access units, ${units.filter((u) => u.key).length} keyframes`)
console.log(`codec string from SPS: ${codec}`)
if (jitterMs) console.log(`artificial send delay: ${jitterMs}ms`)

const MIME = { '.html': 'text/html', '.mjs': 'text/javascript', '.js': 'text/javascript' }

const http = createServer((req, res) => {
  const path = req.url === '/' ? '/index.html' : req.url.split('?')[0]
  try {
    const body = readFileSync(join(here, path))
    res.writeHead(200, { 'content-type': MIME[extname(path)] ?? 'application/octet-stream' })
    res.end(body)
  } catch {
    res.writeHead(404).end('not found')
  }
})

const wss = new WebSocketServer({ server: http })

wss.on('connection', (ws) => {
  console.log('viewer connected')
  ws.send(JSON.stringify({ type: 'config', codec, fps, frames: units.length }))

  let i = 0
  let stopped = false
  const frameMs = 1000 / fps

  const sendOne = () => {
    if (stopped || ws.readyState !== ws.OPEN) return
    const unit = units[i]
    const header = Buffer.alloc(17)
    header[0] = unit.key ? 1 : 0
    header.writeBigUInt64BE(BigInt(Math.round((i * 1e6) / fps)), 1)
    header.writeBigUInt64BE(BigInt(Date.now()), 9)
    ws.send(Buffer.concat([header, unit.data]))
    i = (i + 1) % units.length
    // Loop the clip, but only from a keyframe - restarting mid-GOP gives the
    // decoder references it does not have and produces garbage that looks like
    // a WebCodecs bug.
    if (i !== 0 || units[0].key) return
    while (i < units.length && !units[i].key) i++
  }

  const timer = setInterval(() => {
    if (jitterMs) setTimeout(sendOne, Math.random() * jitterMs)
    else sendOne()
  }, frameMs)

  ws.on('close', () => {
    stopped = true
    clearInterval(timer)
    console.log('viewer disconnected')
  })
})

http.listen(port, () => console.log(`http://localhost:${port}`))
