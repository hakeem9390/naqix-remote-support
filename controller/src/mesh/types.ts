/** Domain types for the controller UI.
 *
 * Deliberately OUR shape, not MeshCentral's wire shape. The adapter in
 * client.ts maps between them, so swapping the engine later (see ADR-001's
 * week-8 decision gate) touches one file rather than every component. */

export type DeviceState = 'online' | 'offline' | 'in-session'

export interface Device {
  id: string
  name: string
  /** ERP tenant this machine belongs to. Technicians think in customers,
   *  not hostnames - this is what the grid groups by. */
  tenant: string
  state: DeviceState
  os: 'windows' | 'macos' | 'linux' | 'unknown'
  osLabel: string
  lastSeen: string | null
  /** Populated only while connected. */
  monitors?: Monitor[]
  /** Set when another technician already holds the machine. */
  heldBy?: string
}

export interface Monitor {
  id: number
  label: string
  width: number
  height: number
  primary: boolean
}

/** Quality is a user-facing intent, not a bitrate. The engine maps it. */
export type QualityMode = 'auto' | 'speed' | 'quality'

export type ConnectionPhase =
  | 'idle'
  | 'connecting'
  | 'authenticating'
  | 'connected'
  | 'reconnecting'
  | 'degraded'
  | 'disconnected'
  | 'failed'

export interface ConnectionState {
  phase: ConnectionPhase
  /** Human-readable reason, shown verbatim on failure. Never swallow this -
   *  a black screen with no explanation is the top support complaint. */
  detail?: string
  latencyMs?: number
  /** Frames per second actually rendered, not requested. */
  fps?: number
  kbps?: number
}

export interface RemoteFile {
  name: string
  path: string
  kind: 'file' | 'directory'
  size: number
  modified: string | null
}

export type TransferDirection = 'upload' | 'download'

export interface TransferJob {
  id: string
  name: string
  direction: TransferDirection
  bytesTotal: number
  bytesDone: number
  state: 'queued' | 'active' | 'done' | 'error' | 'cancelled'
  error?: string
}

/** Pointer/keyboard events, already normalised to remote coordinates. */
export interface PointerEventPayload {
  x: number
  y: number
  button: 0 | 1 | 2
  action: 'move' | 'down' | 'up' | 'wheel'
  wheelDeltaX?: number
  wheelDeltaY?: number
}

export interface KeyEventPayload {
  /** Physical key (KeyboardEvent.code), NOT .key. The remote host applies its
   *  own layout - sending characters breaks on any non-matching layout. */
  code: string
  action: 'down' | 'up'
  modifiers: { alt: boolean; ctrl: boolean; shift: boolean; meta: boolean }
}
