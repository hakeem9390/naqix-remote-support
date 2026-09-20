# Remote Access App

[![Repo](https://img.shields.io/badge/GitHub-naqix--remote--support-181717?logo=github)](https://github.com/hakeem9390/naqix-remote-support)

Unattended remote support into Naqix ERP customers' Windows PCs, plus file
transfer. Controlled from macOS and occasionally a phone.

**Approach:** MeshCentral as the engine, our own React UI as the face, and
time-boxed spikes to close the one real gap (H.264 capture). See
[ADR-001](docs/adr-001-engine-choice.md) for why, and what was rejected.

## Layout

| Path | Track | What |
|---|---|---|
| `deploy/meshcentral/` | A | Docker + nginx stack. [Runbook](deploy/meshcentral/README.md). |
| `controller/` | A2 | React controller UI - the AnyDesk-like face |
| `agent-ui/` | A2 | Customer-side tray app, consent dialog, session indicator |
| `spikes/` | B | Time-boxed risk spikes, each with a kill criterion. [Details](spikes/README.md). |
| `docs/` | - | Decision records |

## Status

- [x] Repo scaffold, deploy stack, ADR
- [x] **Track A2** - controller UI scaffolded on a mock engine (builds, typechecks, verified in-browser)
- [ ] **Track A** - MeshCentral deployed, exit criterion met on a real customer PC
- [ ] **Track A2** - live MeshCentral client behind the `MeshClient` seam
- [ ] **Track B** - spikes run, decision gate reached
- [ ] Week-8 decision: fork MeshAgent with H.264, or accept JPEG for now

## The one thing to keep in view

MeshCentral captures via GDI + tile-based JPEG, not H.264. That is the whole
distance between what Track A ships and the "feels like AnyDesk" bar. Everything
in `spikes/` exists to find out, cheaply and early, whether we can close it by
forking MeshAgent's capture layer while keeping its session-0/UAC handling,
transport and management plane - all of which are already solved and
Apache-2.0.

Do not let Track B turn into a from-scratch rewrite by accident. The spikes
produce a *decision*, not a product.
