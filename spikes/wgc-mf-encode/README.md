# Spike C — WGC → zero-copy D3D11 → hardware H.264

Answers three questions. Nothing else.

1. Can a hardware MFT encode WGC's textures with no CPU round-trip?
2. Does mid-stream bitrate retuning actually take effect? (congestion response)
3. Can a keyframe be forced on demand? (PLI response)

## Run

On Windows, with a GPU that has a hardware H.264 encoder:

```bash
cargo run --release -- out.h264
```

Captures the primary monitor for 15s. At 5s it drops the bitrate to a fifth, at
8s forces a keyframe, at 10s restores the bitrate. It prints a per-second byte
histogram so criterion 2 is judged from *output data*, not from an API call
returning `Ok`.

Then confirm the file actually decodes — the encoder reporting success and the
bytes being valid H.264 are different claims:

```bash
ffprobe -show_frames -select_streams v out.h264 | findstr key_frame
```

## What counts as passing

| | Pass looks like |
|---|---|
| 1 — zero-copy | Runs at capture framerate with low CPU. **Confirm with GPU counters in Task Manager, not the program's output** — it cannot detect a silent CPU fallback. |
| 2 — bitrate | A visible dip in the histogram between 5s and 10s, and recovery after. |
| 3 — keyframe | Keyframe count rises right after the 8s request; `ffprobe` shows `key_frame=1` there. |

Run it on **three machines**: yours, a cheap old Intel iGPU laptop, and one with
no discrete GPU. Your customers do not have RTX 4090s, and this is exactly the
code that behaves differently per vendor.

"No hardware H.264 encoder on this machine" is a **finding, not a bug**. Record
the GPU and move to the next machine.

## Kill criterion

If async MFT sequencing defeats you inside **two weeks**, stop and take the
GStreamer `webrtcsink` + `rtpgccbwe` path, which gives capture, hardware encode
and congestion control as configuration rather than unsafe Rust. Do not keep
going because it feels nearly working — that is the failure mode this spike
exists to prevent.

## Verification status — read this

The code **compiles clean** (`cargo check` and `cargo clippy`, zero warnings)
cross-compiled from macOS against `x86_64-pc-windows-msvc`. That verifies types,
API signatures, feature gates and borrows.

It has **never been linked or run.** Cross-checking cannot catch:

- whether the async MFT event sequencing is correct *at runtime* — the most
  likely place this is wrong, and it manifests as a deadlock rather than an error
- whether zero-copy genuinely happens, or a silent CPU fallback occurs
- driver and vendor quirks, which is most of the real difficulty here
- whether the Video Processor MFT is fast enough to sit in the hot path

Treat the first run as debugging, not as validation.

## Shape of the code

| File | Why it exists |
|---|---|
| `src/encoder.rs` | The hard part. Hardware MFT discovery, D3D11 device binding, async event loop, bitrate and keyframe control. |
| `src/convert.rs` | WGC gives BGRA; every vendor's encoder wants NV12. Converts on-GPU via the Video Processor MFT so the frame never leaves the card. |
| `src/main.rs` | Capture handler, the three criterion exercises, and the report. |

## Things that cost time, written down

- **Output type before input type.** The encoder derives which inputs it admits
  from the output asked for. The other order fails with a misleading error.
- **Async MFTs arrive locked.** Without `MF_TRANSFORM_ASYNC_UNLOCK` they refuse
  everything, and the error does not say why.
- **Never call `ProcessInput` before `METransformNeedInput`.** This is the
  classic bug and it *deadlocks* rather than erroring. `submit()` refuses instead.
- **The encoder must get the same `ID3D11Device` as the capture.** A different
  device still works — over a silent CPU copy, which defeats the whole point.
- **`Win32_System_Ole` is required** for `VARIANT`. Without it `ICodecAPI`'s
  entire impl block is `cfg`'d out and presents as "no method named `SetValue`",
  which sends you hunting in the wrong place.
- **Video processor is synchronous, encoder is not.** `ProcessInput` then
  `ProcessOutput` is correct for one and a deadlock for the other.

## Cross-checking from macOS

Useful while writing, since it catches signature errors without a Windows box:

```bash
rustup target add x86_64-pc-windows-msvc
DEVELOPER_DIR=/Library/Developer/CommandLineTools cargo check --target x86_64-pc-windows-msvc
```

`cargo check` does not link, so no MSVC toolchain is needed. The `DEVELOPER_DIR`
override is only necessary on a Mac where the full Xcode licence has not been
accepted — build scripts still compile for the host and need a working `cc`.
