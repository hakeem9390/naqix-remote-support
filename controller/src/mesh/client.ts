import type {
  ConnectionState, Device, KeyEventPayload, PointerEventPayload,
  QualityMode, RemoteFile, TransferJob,
} from './types'

/** The single seam between UI and engine.
 *
 * Everything the controller needs from MeshCentral goes through here. The UI
 * imports this interface and never MeshCentral types directly, so the week-8
 * decision (keep MeshCentral / fork MeshAgent with H.264 / something else)
 * changes one implementation rather than the app. */
export interface MeshClient {
  listDevices(): Promise<Device[]>
  /** Push updates. Returns an unsubscribe fn. */
  subscribeDevices(cb: (devices: Device[]) => void): () => void

  connect(deviceId: string, opts?: { monitor?: number }): Promise<SessionHandle>
}

export interface SessionHandle {
  readonly deviceId: string

  /** Frames are painted here. The engine owns the surface; the UI only sizes
   *  and positions it. Canvas rather than <video> because MeshCentral's
   *  current KVM path is tile-based JPEG, not a video stream. If Spike C2
   *  lands, this becomes a <video> or a WebCodecs-fed canvas - either way the
   *  UI contract is unchanged. */
  attach(surface: HTMLCanvasElement): void
  detach(): void

  onState(cb: (s: ConnectionState) => void): () => void
  onMonitors(cb: (m: Device['monitors']) => void): () => void

  setMonitor(id: number): void
  setQuality(q: QualityMode): void

  sendPointer(e: PointerEventPayload): void
  sendKey(e: KeyEventPayload): void

  /** Browsers can never capture Ctrl+Alt+Del - it is the Secure Attention
   *  Sequence and the OS takes it below the application layer. The agent has
   *  to raise it via SendSAS on our behalf. */
  sendSAS(): void

  /** Text only. Copying FILES between machines is impossible from a browser;
   *  use the transfer panel for that. */
  syncClipboard(text: string): void

  listFiles(path: string): Promise<RemoteFile[]>
  upload(file: File, remotePath: string): TransferJob
  download(remotePath: string): TransferJob
  onTransfer(cb: (jobs: TransferJob[]) => void): () => void
  cancelTransfer(id: string): void

  disconnect(): void
}

/** Selects the implementation.
 *
 * The real client is not written yet: MeshCentral's KVM wire protocol needs to
 * be read off a running instance rather than guessed at, and the plan is to
 * lift its desktop.js / agent-desktop.js modules (Apache-2.0) rather than
 * reimplement them. Until Track A is deployed, the mock drives the UI so the
 * interface and the screens can be built and reviewed for real. */
export async function createMeshClient(): Promise<MeshClient> {
  const origin = import.meta.env.VITE_MESH_ORIGIN
  if (!origin) {
    const { MockMeshClient } = await import('./mock')
    return new MockMeshClient()
  }
  throw new Error(
    'Live MeshCentral client not implemented yet. See controller/src/mesh/README.md ' +
    'for what has to be read off a running instance first. Unset VITE_MESH_ORIGIN to use the mock.'
  )
}
