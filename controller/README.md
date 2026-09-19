# Controller UI

The operator-facing face of remote support. MeshCentral is the engine; this is
everything the technician actually touches.

```bash
pnpm install
pnpm dev        # http://localhost:5180 - runs on the mock engine
```

No MeshCentral instance is needed to work on this. With `VITE_MESH_ORIGIN`
unset it runs against a synthetic engine (`src/mesh/mock.ts`) that paints a fake
desktop, walks the real connection phases including a degraded spell, and
simulates transfers - successes *and* failures.

## Why there is a mock

`src/mesh/client.ts` defines `MeshClient` / `SessionHandle`, the single seam
between UI and engine. Components never import MeshCentral types, so the week-8
decision in [ADR-001](../docs/adr-001-engine-choice.md) - keep MeshCentral, fork
MeshAgent with H.264, or something else - changes one file instead of the app.

The live client is deliberately unwritten: MeshCentral's KVM wire protocol
should be read off a running instance, not guessed at from documentation. See
[src/mesh/README.md](src/mesh/README.md) for exactly what to capture and which
Apache-2.0 modules to lift rather than reimplement.

## Layout

| Path | What |
|---|---|
| `src/mesh/` | Engine adapter - the interface, and the mock behind it |
| `src/devices/` | Connect bar and tenant-grouped device grid |
| `src/session/` | Canvas surface, input mapping, toolbar, connection banner |
| `src/files/` | Transfer panel |

## Decisions worth not undoing

- **Grouped by tenant, not hostname.** A technician picking up a ticket thinks
  "Acme Pharmacy", never "RECEPTION-PC".
- **`?device=<id>` connects straight through**, skipping the grid. This is the
  deep link from a Naqix support ticket and the main reason to own the
  front-end at all.
- **Keyboard sends `KeyboardEvent.code`, never `.key`.** The remote host applies
  its own layout; sending characters breaks the moment a macOS layout meets a
  Windows one.
- **Coordinate mapping accounts for letterboxing.** The canvas renders with
  `object-contain`, so mapping straight off `getBoundingClientRect` would skew
  every click by the size of the bars.
- **Connection state is never silent.** Failures carry their reason verbatim
  into the banner. A frozen frame with no explanation is the most common
  complaint about tools in this category, and most of what AnyDesk's polish
  actually consists of.
- **Theme is lifted from Naqix** (`ink` light, `slate` dark) so the embedded
  viewer does not look like a foreign app inside the ERP.

## Browser support

Chrome and Edge are the supported controllers. Safari and Firefox have no
`navigator.keyboard.lock()`, so `Cmd+W` / `Cmd+Q` / `Cmd+Tab` hit the
technician's browser instead of the remote PC - the session view detects this
and says so rather than degrading silently.

`showSaveFilePicker` is Chromium-only too; elsewhere a download buffers entirely
in memory, so the transfer panel refuses files over 500 MB with an explanation.

Ctrl+Alt+Del can never be captured by any browser - it is the Secure Attention
Sequence. The toolbar button asks the agent to raise it via `SendSAS`.
