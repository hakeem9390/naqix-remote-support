# Spike C2 — Annex-B H.264 over WebSocket → browser WebCodecs

Answers one question: **can a browser decode our encoder's output over a plain
TCP WebSocket, well enough to control a machine by?**

If yes, the entire WebRTC / ICE / TURN / coturn layer disappears. MeshAgent
already dials outbound, so NAT is solved; we would be adding H.264 to a working
transport rather than building a new one. Spike B becomes unnecessary.

## Run

```bash
npm install
ffmpeg -f lavfi -i "testsrc2=size=1280x720:rate=30:duration=6" \
  -c:v libx264 -profile:v main -preset ultrafast -tune zerolatency -g 60 -bf 0 \
  -bsf:v h264_mp4toannexb -f h264 sample.h264
npm start
```

Open <http://localhost:5190>. Feed it Spike C's `out.h264` instead once that runs.

`JITTER_MS=200 npm start` bunches sends to imitate an unsteady link.

## What passes

| Stat | Pass |
|---|---|
| errors | 0. Anything else means the framing is wrong, not the codec. |
| queue depth | Stays near 0. Growth means the decoder is behind and latency is compounding. |
| frame gap p50 / p95 | p95 within ~3× p50. **This is the one that matters** — smoothness is what a remote desktop is judged on, and an average hides stutter completely. |
| decode latency | Single-digit ms at 720p. |

## Kill criterion

If WebCodecs latency or Safari/mobile support is unacceptable, fall through to
Spike B (WebRTC via webrtc-rs 0.21).

## Results so far

Measured on the Chromium-based browser pane, macOS, localhost, with the ffmpeg
test pattern above:

| | clean | `JITTER_MS=200` |
|---|---|---|
| rendered fps | 30 | 32 |
| frame gap p50 / p95 | 34 / 64 ms | **24 / 100 ms** (flagged) |
| decode latency | 2.2 ms | 2.0 ms |
| transport latency | 1.5 ms | 1.2 ms |
| errors | 0 | 0 |

So the transport and the decode path work, and the stutter metric does
discriminate between a steady and an unsteady sender.

## What this does NOT yet show — read before concluding anything

- **Safari and Firefox are untested.** This is the decisive question and it is
  still open. Chromium working tells you almost nothing about the kill
  criterion. Test both, plus iOS Safari, before deciding.
- **`JITTER_MS` is not TCP head-of-line blocking.** It delays sends at the
  application layer. Real HOL blocking is the kernel withholding *already
  arrived* bytes because an earlier segment was lost, and it cannot be
  reproduced from userspace. Use Network Link Conditioner (macOS) or
  `tc netem loss 2%` (Linux) on the real path. This is the entire reason the
  spike exists, so do not skip it.
- **Localhost latency is not network latency.** 1.5 ms transport is the loopback,
  not a link. Both clocks are also the same clock here, so the transport figure
  is only meaningful within one machine.
- **Not tested against Spike C's output.** The test pattern is synthetic. A
  hardware encoder picks different profiles and levels per vendor, and
  `VideoDecoder.configure` rejects a codec string it does not like outright.
- **No input path.** Viewing only. Round-trip feel is a different measurement.

## Things that cost time, written down

- **Access unit boundaries are `first_mb_in_slice == 0`, not "one VCL NAL".**
  The shortcut is wrong as soon as an encoder emits multiple slices per picture,
  which x264 does under `-tune zerolatency` and hardware encoders do at higher
  resolutions. It does not error — it silently multiplied the frame count by 10
  here, so the stream ran at a tenth speed with no clue as to why. Caught only
  by checking the parser against `ffprobe` rather than trusting it.
- **Leading SPS/PPS must stay attached to the IDR that follows.** Split them off
  as their own access unit and the decoder never gets its parameter sets, so it
  cannot start at all.
- **Build the codec string from the SPS.** Guessing a common one like
  `avc1.42E01E` fails the moment the encoder picks another profile or level.
  Read `profile_idc` / constraint flags / `level_idc` out of the SPS instead.
- **`VideoFrame.close()` is mandatory.** Each frame holds a GPU buffer; leaking
  them stalls the decoder within seconds and presents as "WebCodecs is slow".
- **Never feed delta frames before the first keyframe**, and when looping, restart
  from a keyframe. Otherwise the decoder is handed references it does not have
  and produces garbage that looks like a codec bug.
- **Report a counted framerate, not an exponential average.** Bursty arrivals
  drove the EMA to 334 fps here — an average is unbounded and flattering
  exactly when the link is worst. Count frames in a 1s window and report gap
  percentiles alongside.

## Files

| File | Why |
|---|---|
| `annexb.mjs` | Annex-B parsing. Kept separate because it is the most likely thing to be subtly wrong, and a bug here looks like a decoder problem. Validated against `ffprobe`. |
| `server.mjs` | Deliberately dumb WebSocket sender. Not the point of the spike. |
| `index.html` | The viewer, and the measurements that decide the criterion. |
