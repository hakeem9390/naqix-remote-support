import type { MeshClient, SessionHandle } from './client'
import type {
  ConnectionState, Device, KeyEventPayload, Monitor, PointerEventPayload,
  QualityMode, RemoteFile, TransferJob,
} from './types'

/** Synthetic engine. Exists so the UI can be built and judged before Track A
 *  is deployed. Paints a fake desktop, walks the real connection phases, and
 *  simulates transfers - including failures, which is the half that usually
 *  goes untested. */

const DEVICES: Device[] = [
  { id: 'demo-recep-01', name: 'RECEPTION-PC', tenant: 'Acme Pharmacy', state: 'online', os: 'windows', osLabel: 'Windows 11 Pro 24H2', lastSeen: new Date().toISOString() },
  { id: 'demo-bill-02', name: 'BILLING-02', tenant: 'Acme Pharmacy', state: 'online', os: 'windows', osLabel: 'Windows 10 Pro 22H2', lastSeen: new Date(Date.now() - 4 * 60_000).toISOString() },
  { id: 'demo-store-01', name: 'STORE-BACK', tenant: 'Acme Pharmacy', state: 'in-session', os: 'windows', osLabel: 'Windows 11 Home', lastSeen: new Date().toISOString(), heldBy: 'tech-a' },
  { id: 'demo-front-01', name: 'MVT-COUNTER', tenant: 'Globex Trading', state: 'online', os: 'windows', osLabel: 'Windows 11 Pro', lastSeen: new Date(Date.now() - 90_000).toISOString() },
  { id: 'demo-acct-01', name: 'MVT-ACCOUNTS', tenant: 'Globex Trading', state: 'offline', os: 'windows', osLabel: 'Windows 10 Pro', lastSeen: new Date(Date.now() - 3 * 3600_000).toISOString() },
  { id: 'naqix-build-01', name: 'BUILD-BOX', tenant: 'Naqix Internal', state: 'online', os: 'linux', osLabel: 'Ubuntu 24.04', lastSeen: new Date().toISOString() },
]

const MONITORS: Monitor[] = [
  { id: 0, label: 'Monitor 1', width: 1920, height: 1080, primary: true },
  { id: 1, label: 'Monitor 2', width: 1920, height: 1080, primary: false },
]

const FILES: RemoteFile[] = [
  { name: 'C:\\', path: 'C:\\', kind: 'directory', size: 0, modified: null },
  { name: 'naqix-backup-2026-09-19.sql', path: 'C:\\naqix\\naqix-backup-2026-09-19.sql', kind: 'file', size: 428_337_664, modified: new Date(Date.now() - 7200_000).toISOString() },
  { name: 'gstr1-sep.xlsx', path: 'C:\\naqix\\gstr1-sep.xlsx', kind: 'file', size: 94_208, modified: new Date(Date.now() - 1800_000).toISOString() },
  { name: 'error.log', path: 'C:\\naqix\\error.log', kind: 'file', size: 2_310_144, modified: new Date(Date.now() - 300_000).toISOString() },
  { name: 'logs', path: 'C:\\naqix\\logs', kind: 'directory', size: 0, modified: new Date(Date.now() - 86_400_000).toISOString() },
]

export class MockMeshClient implements MeshClient {
  async listDevices() {
    await delay(320)
    return DEVICES
  }

  subscribeDevices(cb: (d: Device[]) => void) {
    // Flip one machine's presence periodically so the grid proves it is live.
    const t = setInterval(() => {
      const d = DEVICES[3]
      if (d) d.state = d.state === 'online' ? 'offline' : 'online'
      cb([...DEVICES])
    }, 12_000)
    return () => clearInterval(t)
  }

  async connect(deviceId: string): Promise<SessionHandle> {
    return new MockSession(deviceId)
  }
}

class MockSession implements SessionHandle {
  private stateCbs = new Set<(s: ConnectionState) => void>()
  private monitorCbs = new Set<(m: Monitor[]) => void>()
  private transferCbs = new Set<(j: TransferJob[]) => void>()
  private jobs: TransferJob[] = []
  private canvas: HTMLCanvasElement | null = null
  private raf = 0
  private timers: number[] = []
  private cursor = { x: 960, y: 540 }
  private monitor = 0
  private quality: QualityMode = 'auto'
  private disposed = false

  constructor(readonly deviceId: string) {
    this.step('connecting')
    this.after(420, () => this.step('authenticating'))
    this.after(900, () => {
      this.step('connected')
      this.monitorCbs.forEach((cb) => cb(MONITORS))
    })
    // Drop to degraded briefly, then recover. The reconnect path is the one
    // users actually notice, so make it easy to look at during development.
    this.after(14_000, () => this.step('degraded', 'Packet loss on the remote link'))
    this.after(19_000, () => this.step('connected'))
  }

  private after(ms: number, fn: () => void) {
    this.timers.push(window.setTimeout(fn, ms))
  }

  private step(phase: ConnectionState['phase'], detail?: string) {
    if (this.disposed) return
    const degraded = phase === 'degraded'
    const s: ConnectionState = {
      phase,
      detail,
      latencyMs: phase === 'connected' ? 38 + Math.round(Math.random() * 14) : degraded ? 210 : undefined,
      fps: phase === 'connected' ? 28 + Math.round(Math.random() * 4) : degraded ? 7 : undefined,
      kbps: phase === 'connected' ? 1400 + Math.round(Math.random() * 600) : degraded ? 240 : undefined,
    }
    this.stateCbs.forEach((cb) => cb(s))
  }

  attach(surface: HTMLCanvasElement) {
    this.canvas = surface
    const mon = MONITORS[this.monitor] ?? MONITORS[0]!
    surface.width = mon.width
    surface.height = mon.height
    const draw = () => {
      if (this.disposed || !this.canvas) return
      paintFakeDesktop(this.canvas, this.cursor, this.deviceId, this.quality)
      this.raf = requestAnimationFrame(draw)
    }
    draw()
  }

  detach() {
    cancelAnimationFrame(this.raf)
    this.canvas = null
  }

  onState(cb: (s: ConnectionState) => void) {
    this.stateCbs.add(cb)
    return () => this.stateCbs.delete(cb)
  }
  onMonitors(cb: (m: Monitor[] | undefined) => void) {
    this.monitorCbs.add(cb as (m: Monitor[]) => void)
    return () => this.monitorCbs.delete(cb as (m: Monitor[]) => void)
  }
  onTransfer(cb: (j: TransferJob[]) => void) {
    this.transferCbs.add(cb)
    return () => this.transferCbs.delete(cb)
  }

  setMonitor(id: number) {
    this.monitor = id
    if (this.canvas) {
      const mon = MONITORS[id] ?? MONITORS[0]!
      this.canvas.width = mon.width
      this.canvas.height = mon.height
    }
  }
  setQuality(q: QualityMode) { this.quality = q }

  sendPointer(e: PointerEventPayload) {
    if (e.action === 'move' || e.action === 'down') {
      this.cursor = { x: e.x, y: e.y }
    }
  }
  sendKey(_e: KeyEventPayload) { /* no-op in mock */ }
  sendSAS() { /* no-op in mock */ }
  syncClipboard(_text: string) { /* no-op in mock */ }

  async listFiles(_path: string) {
    await delay(260)
    return FILES
  }

  upload(file: File, _remotePath: string) {
    return this.runJob({
      id: crypto.randomUUID(), name: file.name, direction: 'upload',
      bytesTotal: file.size, bytesDone: 0, state: 'queued',
    })
  }

  download(remotePath: string) {
    const f = FILES.find((x) => x.path === remotePath)
    return this.runJob({
      id: crypto.randomUUID(),
      name: f?.name ?? remotePath.split('\\').pop() ?? 'file',
      direction: 'download',
      bytesTotal: f?.size ?? 1_048_576,
      bytesDone: 0,
      state: 'queued',
    })
  }

  private runJob(job: TransferJob) {
    this.jobs = [...this.jobs, job]
    this.emitJobs()
    // ~1.5s ramp, then either completes or fails. Failure is deliberate: the
    // error path is the one that never gets exercised otherwise.
    const willFail = job.bytesTotal > 400_000_000
    const tick = window.setInterval(() => {
      const j = this.jobs.find((x) => x.id === job.id)
      if (!j || j.state === 'cancelled') { clearInterval(tick); return }
      j.state = 'active'
      j.bytesDone = Math.min(j.bytesTotal, j.bytesDone + j.bytesTotal / 18)
      if (j.bytesDone >= j.bytesTotal) {
        clearInterval(tick)
        if (willFail) {
          j.state = 'error'
          j.error = 'Remote refused write: access denied to C:\\naqix'
        } else {
          j.state = 'done'
        }
      }
      this.emitJobs()
    }, 90)
    this.timers.push(tick)
    return job
  }

  cancelTransfer(id: string) {
    const j = this.jobs.find((x) => x.id === id)
    if (j) { j.state = 'cancelled'; this.emitJobs() }
  }

  private emitJobs() {
    const snapshot = this.jobs.map((j) => ({ ...j }))
    this.transferCbs.forEach((cb) => cb(snapshot))
  }

  disconnect() {
    this.disposed = true
    this.timers.forEach(clearTimeout)
    this.timers.forEach(clearInterval)
    cancelAnimationFrame(this.raf)
    this.step('disconnected')
    this.stateCbs.clear()
    this.monitorCbs.clear()
    this.transferCbs.clear()
  }
}

function delay(ms: number) {
  return new Promise((r) => setTimeout(r, ms))
}

/** A recognisably "remote Windows desktop" so layout, scaling and cursor
 *  mapping can be judged without a real agent. */
function paintFakeDesktop(
  c: HTMLCanvasElement, cursor: { x: number; y: number }, deviceId: string, quality: QualityMode,
) {
  const ctx = c.getContext('2d')
  if (!ctx) return
  const { width: w, height: h } = c

  const g = ctx.createLinearGradient(0, 0, w, h)
  g.addColorStop(0, '#0f2027')
  g.addColorStop(1, '#203a43')
  ctx.fillStyle = g
  ctx.fillRect(0, 0, w, h)

  // Window
  const wx = w * 0.14, wy = h * 0.16, ww = w * 0.62, wh = h * 0.6
  ctx.fillStyle = 'rgba(255,255,255,0.96)'
  ctx.fillRect(wx, wy, ww, wh)
  ctx.fillStyle = '#e8e8ea'
  ctx.fillRect(wx, wy, ww, 42)
  ctx.fillStyle = '#3c3c43'
  ctx.font = '20px system-ui, sans-serif'
  ctx.fillText(`Naqix ERP — ${deviceId}`, wx + 16, wy + 28)
  ctx.fillStyle = '#6b6b75'
  ctx.font = '17px system-ui, sans-serif'
  ctx.fillText(new Date().toLocaleTimeString(), wx + 16, wy + 78)
  ctx.fillText(`quality: ${quality}`, wx + 16, wy + 106)
  for (let i = 0; i < 7; i++) {
    ctx.fillStyle = i % 2 ? '#f4f4f6' : '#fafafb'
    ctx.fillRect(wx + 16, wy + 126 + i * 34, ww - 32, 32)
  }

  // Taskbar
  ctx.fillStyle = 'rgba(20,20,26,0.9)'
  ctx.fillRect(0, h - 48, w, 48)

  // Cursor
  ctx.fillStyle = '#fff'
  ctx.strokeStyle = '#000'
  ctx.lineWidth = 1.5
  ctx.beginPath()
  ctx.moveTo(cursor.x, cursor.y)
  ctx.lineTo(cursor.x, cursor.y + 22)
  ctx.lineTo(cursor.x + 6, cursor.y + 16)
  ctx.lineTo(cursor.x + 11, cursor.y + 26)
  ctx.lineTo(cursor.x + 15, cursor.y + 24)
  ctx.lineTo(cursor.x + 10, cursor.y + 14)
  ctx.lineTo(cursor.x + 17, cursor.y + 13)
  ctx.closePath()
  ctx.fill()
  ctx.stroke()
}
