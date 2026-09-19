# ADR-001: Use MeshCentral as the engine, build our own face

**Date:** 2026-09-19
**Status:** Accepted

## Context

We need unattended remote desktop + file transfer into ERP customers' Windows
PCs, behind arbitrary NAT/CGNAT, controlled from macOS and occasionally a phone.
The quality bar is "feels like AnyDesk" - both video smoothness and UI polish.

Building this from scratch is ~32-52 focused engineering weeks (12-20 calendar
months alongside running Naqix). The phase that kills projects like this is not
the codec or WebRTC - it is the Windows service/session-0/UAC layer, which
cannot be tested in CI and fails only on other people's machines.

## Decision

Run two tracks. **MeshCentral as the engine**, **our own React UI as the face**,
and spikes to close the one gap that matters.

### Why MeshCentral

- **Apache-2.0, no CLA.** No AGPL reach into Naqix if we ever bundle this
  commercially, and no relicensing risk.
- **Node.js server** - our strongest stack. We can read and patch it on day one.
- Unattended access, file transfer, terminal, device groups, multi-tenancy,
  session recording, 2FA - all already there.
- `allowFraming` + login tokens let us embed the viewer directly in Naqix.
- Agents dial *outbound* over TLS WebSocket, so CGNAT is a non-issue.
- Actively maintained (v1.2.5, Aug-2026; commits Sep-2026).
- Tactical RMM embeds MeshCentral rather than writing its own remote desktop.

### Rejected

| Option | Why not |
|---|---|
| **Build from scratch now** | 12-20 months leaves customer support unblocked for too long, and the hardest part is already solved in three readable open codebases. |
| **RustDesk** | AGPL-3.0 reaches SaaS use via §13. Every management feature we need - web console, accounts, device groups, audit logs, web client, rebranding - is paywalled in Server Pro. |
| **Apache Guacamole** | guacd dials *outbound to the target*, so there is no path through customer CGNAT. Windows Home has no RDP server, and RDP disconnects the console user. |
| **Sunshine + Moonlight** | No file transfer, no web viewer, no fleet management. GPL-3.0. Its Windows input code moved to a paid source-available licence in Aug-2026. |
| **Tactical RMM** | Not open source; its licence requires written approval for commercial use and bans SaaS delivery. Its remote desktop is embedded MeshCentral anyway. |
| **ControlR** | MIT and architecturally modern (DXGI + WebP), but 233 stars, one human contributor, pre-1.0, current release flagged "for testing only". Revisit in six months. |

## Consequences

### Accepted weakness

MeshCentral captures via **GDI + tile-based JPEG**, not H.264. Fine for ERP
forms and grids; visibly behind AnyDesk on a poor link. This is the single gap
between what we ship and the stated quality bar, and Track B exists to close it.

Its stock web UI is also a dated admin console. We treat MeshCentral as a
headless backend and build the controller in React, lifting its `desktop.js` /
`agent-desktop.js` protocol modules under Apache-2.0 while discarding the
surrounding UI.

### Bus factor

Effectively 1.5 maintainers. Mitigated by Apache-2.0 with distributed copyright
and no CLA - nobody can relicense it out from under us, and a fork is always
available.

### Security posture

MeshCentral has a real CVE history, responsibly handled (CVE-2024-26135,
GHSA-c7hr-448w-65px, CVE-2026-66420 - the last fixed in 1.2.1+). It is also
popular *with attackers* as persistence tooling, so our agent binaries will draw
AV/EDR attention. We therefore pin >= 1.2.5, use a real CA certificate (never
self-signed - that is the CSWSH bypass trigger), run on a dedicated host, and
treat the agent->server channel as untrusted input.

## Open question for the week-8 decision gate

Whether to fork MeshAgent and replace its capture layer with DXGI/WGC + H.264.
Depends on Spike C (hardware encode) and Spike C2 (WebCodecs to the browser).
