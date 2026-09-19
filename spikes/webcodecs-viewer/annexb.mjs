/** Annex-B parsing, kept separate so it can be reasoned about on its own.
 *
 * This is the part most likely to be subtly wrong, and a mistake here looks
 * like a decoder problem rather than a parser problem - which is how people
 * end up blaming WebCodecs for their own framing bug. */

/** Byte offsets of every NAL start code in the buffer. */
export function findStartCodes(buf) {
  const positions = []
  for (let i = 0; i + 2 < buf.length; i++) {
    if (buf[i] === 0 && buf[i + 1] === 0) {
      if (buf[i + 2] === 1) {
        positions.push({ at: i, len: 3 })
        i += 2
      } else if (buf[i + 2] === 0 && i + 3 < buf.length && buf[i + 3] === 1) {
        positions.push({ at: i, len: 4 })
        i += 3
      }
    }
  }
  return positions
}

const NAL_NON_IDR = 1
const NAL_IDR = 5
const NAL_SPS = 7

/** A VCL NAL carries actual picture data; everything else is metadata. */
const isVcl = (t) => t === NAL_NON_IDR || t === NAL_IDR

/** Minimal bit reader, enough for the first field of a slice header. */
class BitReader {
  constructor(buf) { this.buf = buf; this.pos = 0 }
  bit() {
    const byte = this.buf[this.pos >> 3]
    if (byte === undefined) return 0
    const b = (byte >> (7 - (this.pos & 7))) & 1
    this.pos++
    return b
  }
  /** Unsigned Exp-Golomb, the encoding H.264 uses for most header fields. */
  ue() {
    let zeros = 0
    while (this.bit() === 0 && zeros < 32) zeros++
    let value = 0
    for (let i = 0; i < zeros; i++) value = (value << 1) | this.bit()
    return (1 << zeros) - 1 + value
  }
}

/**
 * A picture starts at the slice whose first macroblock is 0.
 *
 * The obvious shortcut - treat every VCL NAL as its own picture - is wrong the
 * moment an encoder emits more than one slice per frame, which x264 does under
 * -tune zerolatency and which hardware encoders do at higher resolutions. It
 * does not error; it silently multiplies the frame count, so the stream plays
 * at a fraction of real speed and the cause is invisible.
 */
function startsNewPicture(buf, nal) {
  if (!isVcl(nal.type)) return false
  // Skip the one-byte NAL header; first_mb_in_slice is the first ue(v).
  const reader = new BitReader(buf.subarray(nal.payload, Math.min(nal.payload + 8, buf.length)))
  return reader.ue() === 0
}

/** Group NAL units into access units - one decodable picture each. */
export function toAccessUnits(buf) {
  const codes = findStartCodes(buf)
  if (codes.length === 0) return []

  const units = []
  for (let i = 0; i < codes.length; i++) {
    const start = codes[i].at
    const end = i + 1 < codes.length ? codes[i + 1].at : buf.length
    const header = start + codes[i].len
    units.push({ start, end, type: buf[header] & 0x1f, payload: header + 1 })
  }

  const access = []
  let current = null
  for (const nal of units) {
    // Only split once the current unit actually holds picture data. Without
    // the hasVcl test the leading SPS/PPS becomes its own picture-less access
    // unit, which both adds a phantom frame and - worse - detaches the
    // parameter sets from the IDR that needs them, so the decoder cannot
    // start.
    if (current && current.hasVcl && startsNewPicture(buf, nal)) {
      access.push(current)
      current = null
    }
    if (!current) current = { start: nal.start, end: nal.end, key: false, hasVcl: false }
    current.end = nal.end
    if (isVcl(nal.type)) current.hasVcl = true
    if (nal.type === NAL_IDR) current.key = true
  }
  if (current) access.push(current)

  return access.map((a) => ({ data: buf.subarray(a.start, a.end), key: a.key }))
}

/**
 * Build the codec string WebCodecs needs, from the SPS.
 *
 * VideoDecoder.configure rejects a wrong codec string outright, and guessing a
 * common one like avc1.42E01E fails the moment the encoder picks a different
 * profile or level - which hardware encoders do, per vendor.
 */
export function codecStringFromSps(buf) {
  const codes = findStartCodes(buf)
  for (let i = 0; i < codes.length; i++) {
    const hdr = codes[i].at + codes[i].len
    if ((buf[hdr] & 0x1f) === NAL_SPS) {
      const profile = buf[hdr + 1]
      const constraints = buf[hdr + 2]
      const level = buf[hdr + 3]
      const hex = (n) => n.toString(16).padStart(2, '0')
      return `avc1.${hex(profile)}${hex(constraints)}${hex(level)}`
    }
  }
  return null
}
