# Risk spikes

Throwaway code. The deliverable is a **decision**, not a product. Each spike has
a kill criterion - honour it rather than sinking another week in.

Run in this order. Stop early if a kill criterion fires.

## Spike C - WGC -> Media Foundation H.264 `[highest value, do first]`

The piece MeshCentral lacks, and the prerequisite for every path forward.

- Capture with `windows-capture` v2.x (carries both WGC and DXGI Desktop Duplication).
- Feed the D3D11 texture **zero-copy** into a hardware encoder found via
  `MFTEnumEx(MFT_ENUM_FLAG_HARDWARE)` with an `IMFDXGIDeviceManager`. Annex-B to a file.
- Prove **dynamic bitrate** (`ICodecAPI` / `CODECAPI_AVEncCommonMeanBitRate`) and
  **forced keyframe** (`CODECAPI_AVEncVideoForceKeyFrame`). Both are required -
  the first for congestion response, the second for PLI.
- Do **not** use `windows-capture`'s built-in encoder; it targets file recording
  and won't give per-frame control.
- Test on three machines: yours, a cheap old Intel iGPU laptop, and one with no
  discrete GPU. Customers do not have RTX 4090s.

**Kill:** if async MFT event sequencing (`METransformNeedInput` /
`METransformHaveOutput`) defeats you in two weeks, switch to GStreamer
`webrtcsink` + `rtpgccbwe` - capture, hardware encode and congestion control as
configuration rather than unsafe Rust.

**Read (do not copy - GPL-3.0):** Sunshine's `src/platform/windows/display.h`
first for the RAM-vs-VRAM design, then `display_base.cpp` for the
`DXGI_ERROR_ACCESS_LOST` / `ACCESS_DENIED` error taxonomy - that taxonomy is most
of the real-world pain. For Rust idiom, RustDesk's `libs/scrap/src/dxgi/`.

## Spike C2 - H.264 to the browser

May make Spike B unnecessary entirely.

- Spike C's Annex-B -> browser `VideoDecoder` (WebCodecs) over a plain WebSocket.
- If it works, the whole WebRTC/ICE/TURN/coturn layer disappears: MeshAgent
  already dials outbound, so NAT is solved, and we'd be adding H.264 to a working
  transport rather than building a new one.
- **Measure, don't assume:** WebSocket is TCP, so head-of-line blocking under
  loss is exactly what makes remote desktop feel bad. Test on a lossy link.
  MeshCentral's optional WebRTC data channel is the best-of-both fallback.

**Kill:** if WebCodecs latency or Safari/mobile support is unacceptable, fall
through to Spike B.

## Spike B - webrtc-rs 0.21 congestion control `[only if C2 fails]`

- v0.21.0 shipped full Google Congestion Control on **2026-09-19**. Days old.
  Before that, missing BWE was a project-killer.
- Pin `webrtc = "=0.21.0"`. API rewritten twice in nine months; pre-1.0.
- Pre-recorded H.264 file -> `TrackLocalStaticSample` -> browser. No capture, no
  encoder. Read `targetBitrate` from stats.
- Impose 2% loss / 80ms RTT / 3Mbps -> 800kbps -> 3Mbps with `clumsy` or `tc netem`.

**Kill:** `targetBitrate` must track the step within ~5s both ways without
oscillating. Note RustDesk - mature, well-resourced, Rust - uses WebRTC only as a
*data channel* transport, pinned to 0.13, keeping media on its own protocol.
Calibrate confidence accordingly.

## Spike A - session 0 / lock screen / UAC `[lowest priority now]`

Was the highest risk; **largely de-risked by choosing MeshCentral**, since
MeshAgent already solves it and is Apache-2.0. Run only to build the
understanding a from-scratch build would need.

The chain: `WTSGetActiveConsoleSessionId` -> `WTSQueryUserToken` (needs
`SeTcbPrivilege`; SYSTEM has it) -> `DuplicateTokenEx` ->
**`SetTokenInformation(TokenSessionId)`** (easy to miss - a session-0 token stays
in session 0 otherwise) -> `CreateEnvironmentBlock` -> `CreateProcessAsUser` with
`si.lpDesktop`.

The secure-desktop trick is one line, in MeshAgent's `microstack/ILibProcessPipe.c`:

```c
if (spawnType == ILibProcessPipe_SpawnTypes_WINLOGON)
{ info.lpDesktop = L"Winsta0\\Winlogon"; }
```

Consumer: `meshcore/KVM/Windows/kvm.c` (`kvm_relay_setup`, `CheckDesktopSwitch`),
~500 readable lines. ControlR's `InputDesktopReporter.cs` is the modern
equivalent. Sunshine's `tools/sunshinesvc.cpp` is the best service-lifecycle
reference (`SERVICE_CONTROL_SESSIONCHANGE`, relaunch on `WTS_CONSOLE_CONNECT`,
job object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`).

Constraints worth knowing before you start:

- **DXGI fails entirely in session 0** (`DXGI_ERROR_NOT_CURRENTLY_AVAILABLE`), so
  the supervisor/session-helper split is mandatory, not a design choice.
- **WGC cannot capture the secure desktop.** It shows up as an "invisible UAC":
  the stream keeps running, the prompt simply isn't in the frames.
- **Do not build on `uiAccess=true`.** Project Zero's Feb-2026 Administrator
  Protection analysis found 9 bypasses via UIAccess, all since fixed. Microsoft
  is actively narrowing that door.

Testing is manual and unavoidable: lock, log out entirely, trigger UAC,
fast-user-switch, reboot-without-login. Instrument the desktop/session state
machine with verbose structured logging from day one - you will be reading logs
from a machine you cannot see.
