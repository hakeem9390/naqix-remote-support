# Engine adapter

`client.ts` defines `MeshClient` / `SessionHandle` - the only seam between the
UI and whatever engine is underneath. Components import from here and never
touch MeshCentral types, so the week-8 decision in
[ADR-001](../../../docs/adr-001-engine-choice.md) changes one file.

## Status

- `mock.ts` - drives the whole UI with a synthetic desktop. Default.
- Live client - **not written.** Deliberately.

## Why the live client is not written yet

MeshCentral's KVM wire protocol is not something to guess at from
documentation. Once Track A is deployed, read it off a running instance:

1. Open a KVM session in MeshCentral's own web UI with devtools recording.
2. Capture the WebSocket frames for: session setup, the tile/JPEG frame format,
   input encoding, monitor enumeration and switching, clipboard, and the file
   transfer channel.
3. Lift `desktop.js` and `agent-desktop.js` from the MeshCentral source
   (Apache-2.0 - keep the licence header) rather than reimplementing the
   decoder. Wrap them behind `SessionHandle`.

Only step 3 is real work; the rest is reading.

## Contract notes worth keeping

- **Keyboard: send `KeyboardEvent.code`, never `.key`.** The remote host
  applies its own layout. Sending characters breaks the moment the technician's
  macOS layout differs from the customer's Windows one.
- **Ctrl+Alt+Del cannot be captured by any browser.** `sendSAS()` asks the
  agent to raise it. The toolbar button is the only way in.
- **`navigator.keyboard.lock()` does not exist in Safari or Firefox.** Without
  it `Cmd+W` / `Cmd+Q` / `Cmd+Tab` hit the technician's browser instead of the
  remote PC. The app warns and asks for Chrome or Edge.
- **Clipboard carries text only.** Copying files between machines is not
  possible from a browser; that is what the transfer panel is for.
- **Surface is a `<canvas>`,** because MeshCentral's KVM is tile-based JPEG. If
  Spike C2 lands, this becomes WebCodecs-fed - the UI contract does not change.
