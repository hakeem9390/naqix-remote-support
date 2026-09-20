# Spike A — lock screen, logged-out, and UAC capture

Answers one question: **can a SYSTEM service obtain frames when no user is
viewing the screen?** Specifically the lock screen, a machine rebooted with no
one logged in, and the UAC secure desktop.

The original plan ranked this most-likely-fatal — no ecosystem support, cannot
be tested in CI, fails only on real machines. Choosing MeshCentral **largely
de-risked it**, since MeshAgent already implements this and is Apache-2.0. This
spike exists to build the understanding a from-scratch build would need, and to
verify that claim rather than take it on faith.

## Kill criterion

If frames cannot be obtained from the lock screen **and** from a logged-out
machine, the unattended value proposition is gone. Stop and self-host RustDesk.

## Safety

Run this only on a throwaway VM or a spare box you own — not a shared machine.
`uninstall` when finished and remove `C:\ProgramData\naqix-spike-a`. The service
runs as LOCAL_SYSTEM because only a LOCAL_SYSTEM process may open the secure
desktop; anything else gets `E_ACCESSDENIED`.

## Run (elevated prompt)

Work up in this order — each step needs less trust than the next:

```
spike-a probe          # print session + desktop state, change nothing
spike-a helper manual   # run the capture side directly, no service
spike-a install         # register + start the auto-start SYSTEM service
spike-a uninstall       # remove it
```

`probe` first, and run it again while locked, to see what the machine reports
before any service exists. Then `helper manual` on the normal desktop to confirm
capture works at all. Only then `install`.

Output: log at `C:\ProgramData\naqix-spike-a\spike.log`, PNGs under `frames\`.
Watch the log with `Get-Content -Wait`.

## The test matrix — the whole point

Capture is unavoidably manual; there is no CI for this. For each state, do the
thing, then check the log for `FIRST FRAME` and confirm a PNG landed:

| State | How | What decides pass/fail |
|---|---|---|
| Normal desktop | just running | frames written — baseline |
| **Locked** | Win+L | frames keep coming from the `Winlogon` desktop |
| **Logged out** | sign out fully | `SESSION CHANGE` logged, helper relaunched, frames from the logon screen |
| **Rebooted, nobody logged in** | reboot, do not sign in | service auto-starts and frames appear — the strongest result |
| UAC prompt | trigger any elevation | see the caveat below |
| Fast user switch | switch users | `SESSION CHANGE`, old helper dropped, new one spawned |

## Set expectations: the UAC secure desktop

Plan for **partial** success. WGC does not error on the secure desktop — it runs
and the prompt is simply **absent from the frames** ("invisible UAC"). The log
will show frames continuing while the actual prompt is missing. Do not read that
as success. This is a known WGC property, not a bug in this code, and the
working approaches (SYSTEM-in-console-session following `Winlogon`) are
version-sensitive. `PromptOnSecureDesktop = 0` renders UAC on the normal desktop
where WGC captures it — a real security reduction, so an informed opt-in, never
silent.

Do **not** build on `uiAccess=true`. Project Zero's Feb-2026 analysis found 9
UIAccess bypasses, all since fixed; Microsoft is narrowing that door.

## Verification status

Compiles clean under `cargo check` and `cargo clippy`, zero warnings,
cross-compiled from macOS against `x86_64-pc-windows-msvc`. **Never linked or
run** — every result in the matrix above is unverified. The token/session/
desktop dance only fails on a real machine, so treat the first run as debugging.

The one thing cross-checking cannot catch and that most needs a real box: the
250ms desktop-switch poll racing the actual switch, and whether
`follow_input_desktop` reliably lands on `Winlogon` before capture reinitialises.

## Shape of the code

| File | Why it exists |
|---|---|
| `launch.rs` | The token dance. `WTSQueryUserToken` → `DuplicateTokenEx` → `SetTokenInformation(TokenSessionId)` → `CreateProcessAsUserW`, plus the SYSTEM-token fallback for when no user is logged in. |
| `desktop.rs` | Which desktop has input, and attaching to it. `OpenInputDesktop` + `SetThreadDesktop`. |
| `service.rs` | The session-0 supervisor. Handles `SESSION_CHANGE`, polls for desktop switches, respawns the helper across the boundary. |
| `helper.rs` | The capture side: one monitor, 5fps, PNGs. Deliberately dumb, so a failure is unmistakably about session/desktop handling and nothing downstream. |
| `log.rs` | File logging to ProgramData. The only instrument when the screen cannot be seen; both processes write one file. |

## Things that cost time, written down

- **`SetTokenInformation(TokenSessionId)` is mandatory and easy to miss.** A
  session-0 token stays in session 0; setting `lpDesktop` alone does not move
  the process, and it then fails in a way that points at the capture code.
- **No logged-in user means no user token.** `WTSQueryUserToken` fails on a
  logged-out machine, so a design depending on it can never capture one.
  Duplicating the service's own SYSTEM token and retargeting the session is the
  only path — and the only one that can open `Winlogon`.
- **`CreateEnvironmentBlock` matters.** Skip it and the child gets no
  `%APPDATA%`/`%TEMP%` and a broken profile, surfacing later as unrelated-looking
  failures.
- **Declare `SERVICE_ACCEPT_SESSIONCHANGE`** or the session notifications never
  arrive.
- **The secure-desktop switch raises no event you can receive** from the losing
  desktop, so the 250ms poll is not laziness — it is what shipping products do.
- **DXGI is dead in session 0** (`DXGI_ERROR_NOT_CURRENTLY_AVAILABLE`), which is
  why the supervisor/helper split is mandatory, not a design choice.

## Reference implementations (all more battle-tested than this)

- MeshAgent `microstack/ILibProcessPipe.c` — Apache-2.0, the one to copy. The
  secure-desktop trick is literally `info.lpDesktop = L"Winsta0\\Winlogon";`.
- RustDesk `src/platform/windows.cc` — the same dance in Rust-adjacent C++.
- Sunshine `tools/sunshinesvc.cpp` — the best service/session lifecycle
  reference (`SERVICE_CONTROL_SESSIONCHANGE`, relaunch on `WTS_CONSOLE_CONNECT`).
