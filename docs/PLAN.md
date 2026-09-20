# Remote Access App — Hybrid Plan

> Snapshot of the working plan as of 2026-09-20. See [ADR-001](adr-001-engine-choice.md) for the engine decision and `spikes/*/README.md` for per-spike results.

## Context

You need to remote into ERP customers' Windows PCs for support, plus move files — today that's unsolved, and customer PCs sit behind arbitrary NAT/CGNAT. You control from macOS and occasionally a phone. Unattended access is required (nobody sitting at the remote PC).

Building an AnyDesk equivalent from scratch is ~32–52 focused engineering weeks — 12–20 calendar months alongside running Naqix. That is too long to leave support unblocked, but you've set an **AnyDesk-grade quality bar**, and the one mature Apache-2.0 option (MeshCentral) is genuinely weak exactly there: its Windows agent captures via **GDI + tile-based JPEG**, not H.264.

So this plan runs two tracks:

- **Track A** — deploy MeshCentral now, embedded in Naqix. Support unblocked in ~1–2 weeks.
- **Track B** — run three de-risking spikes that produce the H.264 capture pipeline MeshCentral lacks.

Track B's output is **not throwaway**. The same WGC→Media Foundation H.264 encoder is the prerequisite for *both* possible endgames: forking MeshAgent to fix its capture, or a full from-scratch build. You get a working tool immediately and real data before committing to a 12-month project.

**The core hypothesis Track B tests:** that MeshAgent's capture layer can be swapped for hardware H.264 while keeping its (already solved) session-0/UAC handling, transport, and entire management plane.

---

## Track A — Deploy MeshCentral (weeks 1–2)

**Why this and not RustDesk:** RustDesk is AGPL-3.0 and every management feature you need — web console, user accounts, device groups, audit logs, web client, rebranding — is paywalled in Server Pro. MeshCentral is Apache-2.0 with no CLA, Node.js (your stack), and Tactical RMM embeds it rather than writing its own.

1. **Provision a dedicated VPS.** Do **not** co-host on the Naqix prod box (`<naqix-prod-host>`). MeshCentral grants SYSTEM-level access to customer machines, has a real CVE history, and is popular with attackers as persistence tooling — the blast radius must not overlap the ERP. Node.js runs fine on generic x86-64, so the x86-64-v2 limitation that blocks `sharp` on the Naqix prod box is not a concern here.
2. **Install ≥ v1.2.5** (earlier versions miss the CVE-2026-66420 CSWSH bypass fix). PostgreSQL backend rather than the NeDB default; nginx in front; a **real CA cert, not self-signed** — self-signed certs are what trigger the CSWSH bypass.
3. **Configure per-customer isolation** using device groups, one per ERP tenant, with per-group permissions. Use *domains* if you ever need separate admin identities per customer.
4. **Embed in Naqix.** Set `allowFraming: true`, enable login tokens, and deep-link the viewer per device (`?viewmode=10&gotodevicename=...`). This turns remote support into a button inside the ERP rather than a separate tool — the single highest-value integration, and Apache-2.0 permits it commercially.
5. **Harden:** TOTP 2FA on every operator account, session recording on, agent→server channel treated as untrusted input.
6. **Buy an EV/OV code signing certificate now** (~$300–500/yr). SmartScreen reputation accrues over wall-clock weeks and cannot be compressed later — you need it whichever endgame you pick.

**Exit criterion:** you can connect unattended into a real customer PC from Chrome on your Mac, transfer a file, and survive the remote user locking their screen.

---

## Track A2 — Your own UI (weeks 2–5)

**The problem:** MeshCentral's stock web UI is a dated, utilitarian admin console — dense tables, 2010s styling. It works, but it does not feel like AnyDesk, and you've set that as the bar.

**The fix is the best structural property of this whole plan: decouple the face from the engine.** MeshCentral is Apache-2.0 and speaks WebSocket, so treat it as a *headless backend* and build the controller UI yourself in React — your strongest skill, and the part where your effort actually shows.

**What to reuse vs. rewrite:**

- **Lift the protocol client, rewrite the chrome.** MeshCentral's `desktop.js` / `agent-desktop.js` browser modules already speak its KVM protocol (frame decode, input encoding, monitor switching). Take those modules under Apache-2.0, wrap them in your own React components, and throw away the surrounding UI entirely. You are not re-implementing the protocol — only its presentation.
- Drive device listing, session auth and file transfer through MeshCentral's WebSocket/REST API rather than its pages.

**The two UIs you need:**

**1. Controller (React, inside Naqix or standalone)**
- **AnyDesk's signature interaction:** a large, uncluttered connect field plus a recent-devices grid — not a tree of device groups. Make the common case one click.
- Device cards showing customer/tenant, online state, OS, last seen — with tenant grouping drawn from Naqix, so a technician sees *customers*, not hostnames.
- **Session window:** floating toolbar, monitor switcher, quality toggle (auto / speed / quality), fullscreen, Ctrl+Alt+Del button, file-transfer panel as a slide-over two-pane view.
- Dark mode, proper design tokens, real loading and reconnect states. Connection state must always be legible — AnyDesk's polish is mostly that it never leaves you guessing.
- **Deep-link from a support ticket straight into a session.** This is the thing no off-the-shelf tool gives you, and the strongest argument for owning the front-end.

**2. Agent-side UI (on the customer's Windows PC) — do not skip this**
This is what your *customers* see, and it decides whether they trust the install. MeshAgent's is minimal.
- A clean installer carrying your branding, and a tray app showing connection state.
- A connection-consent dialog (for attended sessions) and the persistent "session active — <operator>" indicator required by the security section above.
- Trustworthy visual design here is not cosmetic: an unsigned, unbranded, SYSTEM-level agent asking for unattended access is indistinguishable from malware to a cautious customer.

**Sequencing note:** start A2 once Track A proves the engine works end-to-end, and run it *concurrently with Track B*. The UI work is in your strongest language and doesn't compete for the same mental context as unsafe Rust/Win32. It also survives every branch of the decision gate — whichever engine you end up on, the React controller and the agent-side UX carry over unchanged.

---

## Track B — Risk spikes (weeks 3–8)

Three independent spikes, each with a **kill criterion**. Throw all the code away afterwards; the output is a decision, not a product. Do them in this order.

> **Status — all four spikes written (2026-09-20).** Committed under `spikes/`. Two ran, two compile-check only because they need Windows hardware I don't have. **No kill criterion is settled yet** — "written" is not "decision reached". What each still needs is under its heading.
>
> | Spike | Code | Verified | Still open |
> |---|---|---|---|
> | **C** — WGC → MF H.264 | ✅ written | `cargo check` + `clippy` clean, cross-compiled to `x86_64-pc-windows-msvc`. **Never linked or run.** | Everything — runtime async-MFT sequencing (the likely failure, deadlocks not errors), real zero-copy vs silent CPU fallback, per-vendor drivers. Needs a Windows box. |
> | **C2** — WebCodecs/WS | ✅ written | **Built + run.** Chromium: 30fps, ~2ms decode, 0 errors. Annex-B parser validated against ffprobe. | **Safari + Firefox untested — the decisive question.** Real TCP head-of-line blocking (needs Network Link Conditioner / `tc netem`, not the app-layer jitter knob I used). |
> | **B** — webrtc-rs GCC | ✅ written | **Built + run.** GCC climbs 800k→5M, 0 downward steps — rules out the stuck-estimator regression (issue #99). | The kill criterion itself: the **downward** step + recovery, which loopback can't produce. Needs a constrained path. |
> | **A** — session-0 / UAC | ✅ written | `cargo check` + `clippy` clean, cross-compiled. **Never linked or run.** | The whole manual matrix (lock / logout / reboot-no-login / UAC / fast-user-switch). Only fails on a real machine. Needs a throwaway Windows VM. |
>
> **Cheapest highest-leverage next step:** run C2 in Safari. A pass there could retire Spike B entirely.

### Spike C (first, highest value) — WGC → Media Foundation H.264

**Status: written, compile-checked only.** `spikes/wgc-mf-encode/`. Needs a Windows box to link and run; treat the first run as debugging. The traps that already cost time (output type before input, async-MFT unlock, `Win32_System_Ole` for `VARIANT`) are in its README.

This is the piece MeshCentral lacks and the prerequisite for every path forward.

- Capture with `windows-capture` v2.x (has both WGC and DXGI Desktop Duplication).
- Feed the D3D11 texture **zero-copy** into a hardware encoder found via `MFTEnumEx(MFT_ENUM_FLAG_HARDWARE)` with an `IMFDXGIDeviceManager`. Write Annex-B to a file.
- Prove **dynamic bitrate** (`ICodecAPI` / `CODECAPI_AVEncCommonMeanBitRate`) and **forced keyframe** (`CODECAPI_AVEncVideoForceKeyFrame`). You need both — the first for congestion response, the second for PLI.
- Do **not** use `windows-capture`'s built-in encoder; it targets file recording and won't give you per-frame control.
- Test on three machines: yours, a **cheap old Intel iGPU laptop**, and one with no discrete GPU. Your customers do not have RTX 4090s.

**Kill criterion:** if async MFT event sequencing (`METransformNeedInput`/`METransformHaveOutput`) defeats you in two weeks, switch to GStreamer `webrtcsink` + `rtpgccbwe`, which gives capture + hardware encode + working congestion control as configuration rather than unsafe Rust.

**Reference to read (do not copy — GPL-3.0):** Sunshine's [`src/platform/windows/display.h`](https://github.com/LizardByte/Sunshine/blob/master/src/platform/windows/display.h) first for the RAM-vs-VRAM design, then `display_base.cpp` for the `DXGI_ERROR_ACCESS_LOST` / `ACCESS_DENIED` error taxonomy — that taxonomy is most of the real-world pain. For Rust idiom, RustDesk's [`libs/scrap/src/dxgi/`](https://github.com/rustdesk/rustdesk/tree/master/libs/scrap).

### Spike C2 — H.264 to the browser

**Status: written, built + run — Chromium passes, Safari/Firefox still the open question.** `spikes/webcodecs-viewer/`. Runs on any machine (takes a pre-recorded file). Decode path and transport confirmed on Chromium; the parser was validated against ffprobe, which caught a real multi-slice bug. Not yet tested against real TCP head-of-line blocking.

Immediately after C, and it may make Spike B unnecessary.

- Feed Spike C's Annex-B stream to a browser `VideoDecoder` via the **WebCodecs API**, over a plain WebSocket first.
- If that works, the entire WebRTC/ICE/TURN/coturn layer disappears — MeshAgent already dials outbound, so NAT is solved, and you'd be adding H.264 to an existing working transport rather than building a new one.
- **Trade-off to measure, not assume:** WebSocket is TCP, so head-of-line blocking under packet loss is exactly what makes remote desktop feel bad on poor links. Measure on a lossy link before accepting it. MeshCentral already has optional WebRTC data channels — H.264 frames over that channel is the best-of-both fallback.

**Kill criterion:** if WebCodecs latency or Safari/mobile support is unacceptable, fall through to Spike B.

### Spike B — webrtc-rs 0.21 congestion control (only if C2 fails)

**Status: written, built + run — wiring confirmed, step response still open.** `spikes/webrtc-gcc/`. The estimator is live and climbs correctly (rules out the stuck-estimator regression). Loopback has no bandwidth limit, so the actual kill criterion — the downward step and recovery — needs a constrained path; the `dnctl`/`pfctl` and `tc netem` invocations are in its README. Key finding: the estimate is *pushed* (wrap the estimator), not pulled — there's no getter.

- `webrtc` v0.21.0 shipped a full Google Congestion Control implementation (Kalman filter, adaptive overuse threshold, AIMD) **on 2026-09-19** — literally days old. Before that, the missing BWE was a project-killer.
- Pin `webrtc = "=0.21.0"` exactly. The crate's API was rewritten twice in nine months and it's pre-1.0.
- Feed a **pre-recorded** H.264 file to `TrackLocalStaticSample` → browser. No capture, no encoder. Read `targetBitrate` from stats.
- Impose 2% loss / 80 ms RTT / a 3 Mbps → 800 kbps → 3 Mbps step with `clumsy` or `tc netem`.

**Kill criterion:** `targetBitrate` must track the step within ~5s in both directions without oscillation. Note that RustDesk — a mature, well-resourced Rust project — uses WebRTC only as a *data channel* transport, pinned to 0.13, and keeps media on its own protocol. Calibrate your confidence accordingly.

### Spike A — session 0 / lock screen / UAC (lowest priority now)

**Status: written, compile-checked only.** `spikes/session0-helper/`. A session-0 supervisor service + a capture helper, with the full token dance. Needs a throwaway Windows VM; the whole manual test matrix is unverified. Expect *partial* success at the UAC secure desktop — WGC doesn't error there, it silently omits the prompt from the frames, so a clean-looking log is not a pass. Matrix and caveats in its README.

Originally the highest-risk item; **largely de-risked by the MeshCentral decision**, since MeshAgent already solves it and is Apache-2.0. Run it only to build the understanding you'd need for a from-scratch build.

The chain: `WTSGetActiveConsoleSessionId` → `WTSQueryUserToken` (needs `SeTcbPrivilege`; SYSTEM has it) → `DuplicateTokenEx` → **`SetTokenInformation(TokenSessionId)`** (easy to miss — a session-0 token stays in session 0 otherwise) → `CreateEnvironmentBlock` → `CreateProcessAsUser` with `si.lpDesktop`.

The secure-desktop trick is a one-liner, visible in MeshAgent's [`ILibProcessPipe.c`](https://github.com/Ylianst/MeshAgent/blob/master/microstack/ILibProcessPipe.c):

```c
if (spawnType == ILibProcessPipe_SpawnTypes_WINLOGON)
{ info.lpDesktop = L"Winsta0\\Winlogon"; }
```

Its consumer is [`meshcore/KVM/Windows/kvm.c`](https://github.com/Ylianst/MeshAgent/blob/master/meshcore/KVM/Windows/kvm.c) (`kvm_relay_setup`, `CheckDesktopSwitch`), ~500 readable lines. ControlR's `InputDesktopReporter.cs` is the most modern equivalent; Sunshine's [`tools/sunshinesvc.cpp`](https://github.com/LizardByte/Sunshine/blob/master/tools/sunshinesvc.cpp) is the best service-lifecycle reference (`SERVICE_CONTROL_SESSIONCHANGE`, relaunch on `WTS_CONSOLE_CONNECT`, job object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`).

Key constraints: **DXGI fails entirely in session 0** (`DXGI_ERROR_NOT_CURRENTLY_AVAILABLE`), so the supervisor/session-helper split is mandatory, not a design choice. **WGC cannot capture the secure desktop** — it manifests as an "invisible UAC" where the stream runs but the prompt isn't in the frames. Do **not** build on `uiAccess=true`; Project Zero's Feb-2026 Administrator Protection analysis found 9 bypasses via UIAccess, all since fixed — Microsoft is actively narrowing that door.

---

## Decision gate (end of week 8)

Pick one, with data rather than intuition:

| Outcome | Next step |
|---|---|
| Spike C + C2 both pass | **Fork MeshAgent**, replace GDI/JPEG capture with DXGI/WGC + H.264. Keep its session handling, transport, and entire management plane. ~2–3 months, and you get AnyDesk-grade video with a working product around it. **Most likely best outcome.** |
| C passes, C2 fails, B passes | Fork MeshAgent but carry H.264 over its WebRTC data channel. |
| C fails | Take the GStreamer path for media, or accept MeshCentral's JPEG quality and revisit in a year. |
| All pass and you still want to own it | Full from-scratch build, with the phase order below. |

**Full-build phase order, if it comes to that** (risk front-loaded): walking skeleton → input (`SendInput` directly, not `enigo` — you need `KEYEVENTF_SCANCODE` for guest keyboard layouts and `MOUSEEVENTF_ABSOLUTE|VIRTUALDESK` for multi-monitor) → signalling/control plane in TypeScript → **service/session/UAC** (the killer phase: untestable in CI, fails only on customer machines, budget 6–12 weeks) → security → file transfer → packaging → hardening.

---

## Security requirements (apply to both tracks)

An unattended agent running as SYSTEM on third-party machines is functionally a RAT. These are first-class deliverables, not polish:

- **Device enrollment, not shared passwords.** Single-use short-lived enrollment token per install; agent generates an Ed25519 identity keypair stored in DPAPI machine scope (or TPM-backed via CNG). Authorisation is then operator-identity based with MFA — no extractable secret on the customer machine grants access to anything.
- **Untrusted relay.** The agent signs its DTLS/TLS fingerprint with its identity key; the controller verifies against a pinned key before `setRemoteDescription` and **fails closed**. This is what makes self-hosting defensible — your VPS can deny service but cannot read or inject. (RustDesk's PR #15684 does exactly this.)
- **Never ship static TURN credentials in JavaScript.** If you end up needing coturn, use time-limited REST credentials minted per session.
- **Consent and visibility:** persistent on-screen indicator naming the operator during an active session, audible session-start cue, and a local-user kill button that works *even while you hold input*. These are what distinguish your product from malware — to customers, to Defender, and in a dispute.
- **Tamper-evident audit log**, hash-chained locally and mirrored server-side, covering session start/end, operator identity, peer IP, and every file transferred (name, size, direction, hash). Give customers read access — for an ERP vendor this is a sales asset, not overhead.
- **Explicit written unattended-access consent** captured at install with timestamp and consenting person's name, and reflected in your support agreement. This matters given the UAE/Saudi PDPL exposure already tracked for Naqix.
- **Path validation on file transfer.** The agent runs as SYSTEM; a path-traversal bug there is total machine compromise.
- **Expect AV/EDR flagging.** Sign every binary and the installer, submit false-positive reports to Microsoft and major AV vendors, and keep a stable publisher identity — rotating certs resets SmartScreen reputation.

---

## Browser controller constraints (affects Track A immediately)

You control from macOS, so this matters now:

- **Safari is not a usable controller.** `navigator.keyboard.lock()` has no support in Safari or Firefox, so `Cmd+W`/`Cmd+Q`/`Cmd+Tab` hit your browser instead of the remote PC. **Mandate Chrome or Edge** and document it.
- **Ctrl+Alt+Del can never be captured by any browser** — it's the Secure Attention Sequence. Needs a toolbar button that tells the agent to call `SendSAS` (works from LocalSystem). MeshCentral already has this.
- **File pickers (`showSaveFilePicker`) are Chromium-only.** On Safari/iOS the whole file buffers in memory — a 2 GB backup kills the tab. Don't treat iOS as a file-transfer client.
- **Clipboard:** text syncs fine; copying *files* between machines is impossible in a browser. For ERP support (invoice numbers, SQL, stack traces) text is 95% of the value.
- **Latency:** expect 90–150 ms over the internet, which is fine for forms and grids. The highest-leverage knob is `RTCRtpReceiver.jitterBufferTarget` at 30–50 ms — do **not** set it to 0, that causes constant stutter.
- **Escape hatch:** if browser limits bite, wrap the same web app in Tauri for macOS — raw keyboard and real filesystem access, reusing ~95% of the code. This is what makes web-first a safe default.

---

## Files — created so far

`[x]` exists and is committed; `[ ]` is still to build.

```
/Users/hakkim/Projects/Remote Access App/
  [x] Cargo.toml                          # workspace; webrtc/windows/windows-capture/windows-service pinned
  [x] spikes/wgc-mf-encode/               # Spike C  — capture → zero-copy MF H.264 → file  (compile-checked)
  [x] spikes/webcodecs-viewer/            # Spike C2 — WebSocket → VideoDecoder → <canvas>  (run, Chromium)
  [x] spikes/webrtc-gcc/                  # Spike B  — pre-recorded H.264 → track → browser (run)
  [x] spikes/session0-helper/             # Spike A  — service + helper launch, PNG dump    (compile-checked)
  [x] deploy/meshcentral/                 # Track A  — docker-compose, nginx, config.json
  [x] controller/                         # Track A2 — React controller UI, on a mock engine
      [x] src/{session,devices,files}/    #   viewer shell, device grid, transfer panel
      [x] src/mesh/                       #   MeshClient seam + mock; live client deliberately unwritten
  [ ] agent-ui/                           # Track A2 — tray app, consent dialog, session indicator (not started)
```

Still unbuilt beyond `agent-ui/`: the **live MeshCentral client** behind the `MeshClient` seam (blocked on a running MeshCentral instance to read the KVM protocol off — see `controller/src/mesh/README.md`).

---

## Verification

**Track A** — on a real customer PC, not a VM: connect unattended from Chrome/macOS; transfer a file both directions; have the user lock the screen mid-session and confirm the stream survives; trigger a UAC prompt and confirm the fallback UX is comprehensible; reboot the machine and confirm the agent reconnects with nobody logged in; confirm the embedded Naqix iframe deep-links to the right device.

**Track A2** — connect to a device in three clicks or fewer from a cold start; switch monitors and transfer a file without leaving the session window; pull the network mid-session and confirm the reconnect state is legible rather than a frozen frame; check the whole flow at phone width. Then have someone who isn't you install the agent on a real machine and watch whether they hesitate — that hesitation is the agent-side UX bug list.

**Spike C** — inspect the Annex-B output in a player; confirm the bitrate actually changes when you retune `CODECAPI_AVEncCommonMeanBitRate` mid-stream; confirm a forced keyframe appears on demand; run all three on the old iGPU laptop, since that machine — not yours — represents your customers.

**Spike C2** — measure glass-to-glass latency with a stopwatch app on the remote screen; then repeat over `clumsy` at 2% loss and judge whether TCP head-of-line blocking is tolerable.

**Spike B** — log `targetBitrate` under the netem bandwidth step; it must converge within ~5s each way without oscillating.

**Spike A** — the real test is manual and unavoidable: lock, log out entirely, trigger UAC, fast-user-switch, and reboot-without-login, checking for frames at each stage. Instrument the desktop/session state machine with verbose structured logging from day one — you will be reading logs from a machine you cannot see.

---

## Explicitly out of scope

- RustDesk (AGPL-3.0 reaches SaaS use via §13, and every management feature is paywalled in Server Pro).
- Apache Guacamole (guacd dials *outbound to the target* — no path through customer CGNAT; also Windows Home has no RDP server, and RDP disconnects the console user).
- Tactical RMM (not open source; its licence requires written approval for commercial use and bans SaaS delivery — and its remote desktop is just embedded MeshCentral anyway).
- Copying Sunshine code (GPL-3.0 — read for domain knowledge only). Note its Windows `SendInput` code moved to the paid source-available `libvirtualhid` in Aug-2026; pin an older tag if you want to read it.
- Supporting RDP sessions on the remote end in v1 — DXGI Desktop Duplication is unavailable inside RDP; report a clear status instead of half-working.
- H.264 patent licensing (MPEG-LA) — worth a look before commercial distribution, but not a build blocker.
