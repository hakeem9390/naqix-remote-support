# Spike B — does webrtc-rs 0.21's congestion control actually converge?

**Only run this if Spike C2 fails.** C2's WebSocket + WebCodecs path, if it
holds up, removes the need for WebRTC entirely — MeshAgent already dials
outbound, so NAT is already solved.

No capture, no encoder: a pre-recorded Annex-B file goes to a browser so the
only thing under test is the estimator. GCC landed in webrtc-rs **0.21.0 on
2026-09-19**, days before this was written. "The API exists" and "the algorithm
converges" are different claims, and this spike exists to separate them.

## Run

```bash
ffmpeg -f lavfi -i "testsrc2=size=1280x720:rate=30:duration=10" \
  -c:v libx264 -profile:v baseline -preset veryfast -tune zerolatency \
  -b:v 2500k -maxrate 2500k -bufsize 1000k -g 60 -bf 0 -x264-params sliced-threads=0 \
  -bsf:v h264_mp4toannexb -f h264 sample.h264

cargo run --release -p webrtc-gcc -- sample.h264
```

Open <http://localhost:5191>. The sender prints its target bitrate twice a
second and writes `gcc-trace.csv`.

Bounds are overridable: `INITIAL_BPS`, `MIN_BPS`, `MAX_BPS`.

## Kill criterion

Target bitrate must follow a **3 Mbps → 800 kbps → 3 Mbps** step within ~5s in
**both** directions, without oscillating. Judge the trace, not whether the
program ran.

## Results so far

Built and run on macOS against Chromium, over loopback, with the file above.

**Connection and media path: works.** Connected, 1502 kbps received, 300 access
units parsed (matching `ffprobe`), 0 packets lost, 0 dropped frames, 0 PLI.

**The estimator is live** — this is the part worth caring about:

```
  0.0s   800 kbps   (initial)
  8.0s  1502 kbps
 17.0s  3002 kbps
 46.5s  5000 kbps   (MAX_BPS ceiling, held)
```

114 samples, **0 downward steps**, smooth monotonic climb to the ceiling.

That result matters because of a specific historical failure: webrtc-rs
[issue #99](https://github.com/webrtc-rs/webrtc/issues/99) had bandwidth
estimation stuck near its starting rate because RTCP feedback was never wired
through, so the estimator held its initial value forever. **That is not
happening here.** TWCC feedback is arriving and driving the estimator, and on an
unconstrained path it probes upward and settles at the ceiling, which is
correct behaviour.

## What this does NOT show — the kill criterion is still open

**Loopback has no bandwidth limit, so there is no congestion to respond to.**
Everything above tests the *upward* probe on a clean path. The kill criterion is
about the **downward** step and the recovery after it, and neither has been
tested. A monotonic climb to the ceiling is consistent both with a healthy
estimator and with one that cannot detect congestion at all — the two are
indistinguishable without a constrained path.

To close it out, constrain the path and re-run. Both need admin rights:

**macOS** (dummynet via pf):
```bash
sudo dnctl pipe 1 config bw 800Kbit/s delay 40ms plr 0.02
echo "dummynet out proto udp from any to any pipe 1" | sudo pfctl -f - -e
# ... observe the drop, then release and watch recovery ...
sudo pfctl -d && sudo dnctl -q flush
```

**Linux**:
```bash
sudo tc qdisc add dev lo root netem rate 800kbit delay 40ms loss 2%
sudo tc qdisc del dev lo root
```

Watch for: does the target fall within ~5s of the constraint, does it recover
within ~5s of removal, and does it oscillate at the boundary rather than settle.

Also untested: sustained multi-minute behaviour, multiple concurrent
connections, and any real network path.

## Note on scope

The sender streams a **fixed-bitrate** file and does not adapt to the estimate.
That is deliberate — this spike measures whether the *number* is right, not
whether something acts on it. The crate's own
`bandwidth-estimation-from-disk` example shows the acting-on-it half by
switching between three pre-encoded renditions, and is worth reading if this
criterion passes.

## Things that cost time, written down

- **`PeerConnection` is a trait and `build()` returns an opaque `impl`.** Without
  importing the trait, every method on the connection reports as "not found",
  which reads like a version mismatch rather than a missing import.
- **The estimate is pushed, not pulled.** `configure_congestion_control` takes
  the estimator by value and boxes it inside the interceptor chain, so there is
  no getter. Wrap the estimator and publish from `on_reports` / `handle_timeout`
  — the two points the contract names as able to move the number. There is no
  callback API to look for.
- **`configure_congestion_control` registers the transport-cc feedback and header
  extension.** Skip it and the remote never reports arrivals, so the estimator
  holds its initial rate — which looks exactly like "GCC is broken".
- **The browser must be `recvonly` and it must send TWCC.** The sender's
  estimator runs entirely on the receiver's feedback.
- **Send whole access units, not individual NALs.** One sample per NAL
  multiplies the send rate by the slice count, and the estimator then measures
  our framing bug instead of the network. Same parser logic as
  `spikes/webcodecs-viewer/annexb.mjs`, validated against `ffprobe`.
- **`default_runtime()` returns a zero-sized adapter**, not a new runtime, so it
  is safe to call inside `#[tokio::main]`.
