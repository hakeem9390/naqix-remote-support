//! Annex-B access unit splitting.
//!
//! Same logic as the JS parser in spikes/webcodecs-viewer/annexb.mjs, which was
//! validated against ffprobe. Reimplemented here rather than shared because the
//! spikes are meant to be independently disposable.
//!
//! rtc-media ships an `h26x_reader`, but it yields NAL units, and pacing needs
//! whole pictures - sending one sample per NAL would multiply the send rate by
//! the slice count and make the estimator measure our framing bug rather than
//! the network.

use bytes::Bytes;

pub struct AccessUnit {
    pub data: Bytes,
    pub key: bool,
}

const NAL_NON_IDR: u8 = 1;
const NAL_IDR: u8 = 5;

fn is_vcl(t: u8) -> bool {
    t == NAL_NON_IDR || t == NAL_IDR
}

struct BitReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> BitReader<'a> {
    fn bit(&mut self) -> u32 {
        let byte = match self.buf.get(self.pos >> 3) {
            Some(b) => *b,
            None => return 0,
        };
        let b = ((byte >> (7 - (self.pos & 7))) & 1) as u32;
        self.pos += 1;
        b
    }
    /// Unsigned Exp-Golomb, the encoding H.264 uses for most header fields.
    fn ue(&mut self) -> u32 {
        let mut zeros = 0;
        while self.bit() == 0 && zeros < 32 {
            zeros += 1;
        }
        let mut value = 0u32;
        for _ in 0..zeros {
            value = (value << 1) | self.bit();
        }
        (1u32 << zeros) - 1 + value
    }
}

struct Nal {
    start: usize,
    end: usize,
    kind: u8,
    payload: usize,
}

fn find_nals(buf: &[u8]) -> Vec<Nal> {
    let mut marks: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    while i + 2 < buf.len() {
        if buf[i] == 0 && buf[i + 1] == 0 {
            if buf[i + 2] == 1 {
                marks.push((i, 3));
                i += 3;
                continue;
            } else if buf[i + 2] == 0 && i + 3 < buf.len() && buf[i + 3] == 1 {
                marks.push((i, 4));
                i += 4;
                continue;
            }
        }
        i += 1;
    }

    marks
        .iter()
        .enumerate()
        .map(|(n, &(at, len))| {
            let end = marks.get(n + 1).map(|m| m.0).unwrap_or(buf.len());
            let header = at + len;
            Nal { start: at, end, kind: buf[header] & 0x1f, payload: header + 1 }
        })
        .collect()
}

/// A picture starts at the slice whose first macroblock is 0.
///
/// Treating every VCL NAL as its own picture is wrong the moment an encoder
/// emits multiple slices per frame - it does not error, it silently multiplies
/// the frame count, which here would corrupt the pacing the estimator is
/// measuring against.
fn starts_new_picture(buf: &[u8], nal: &Nal) -> bool {
    if !is_vcl(nal.kind) {
        return false;
    }
    let end = (nal.payload + 8).min(buf.len());
    let mut reader = BitReader { buf: &buf[nal.payload..end], pos: 0 };
    reader.ue() == 0
}

pub fn to_access_units(buf: &[u8]) -> Vec<AccessUnit> {
    let nals = find_nals(buf);
    if nals.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<AccessUnit> = Vec::new();
    let mut cur: Option<(usize, usize, bool, bool)> = None; // start, end, key, has_vcl

    for nal in &nals {
        if let Some((start, end, key, has_vcl)) = cur
            && has_vcl
            && starts_new_picture(buf, nal)
        {
            out.push(AccessUnit { data: Bytes::copy_from_slice(&buf[start..end]), key });
            cur = None;
        }
        let entry = cur.get_or_insert((nal.start, nal.end, false, false));
        entry.1 = nal.end;
        if is_vcl(nal.kind) {
            entry.3 = true;
        }
        if nal.kind == NAL_IDR {
            entry.2 = true;
        }
    }

    // Leading SPS/PPS must stay attached to the IDR that follows: split off on
    // their own, the decoder never receives its parameter sets and cannot start.
    if let Some((start, end, key, _)) = cur {
        out.push(AccessUnit { data: Bytes::copy_from_slice(&buf[start..end]), key });
    }
    out
}
