import { useCallback, useEffect, useRef, useState } from 'react'
import { TriangleAlert } from 'lucide-react'
import type { SessionHandle } from '@/mesh/client'
import type { ConnectionState, Monitor, QualityMode } from '@/mesh/types'
import { ConnectionBanner } from './ConnectionBanner'
import { SessionToolbar } from './SessionToolbar'
import { useKeyboardLock } from './useKeyboardLock'
import { FileTransferPanel } from '@/files/FileTransferPanel'

export function SessionView({
  session, deviceName, onExit,
}: { session: SessionHandle; deviceName: string; onExit: () => void }) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const shellRef = useRef<HTMLDivElement>(null)
  const [state, setState] = useState<ConnectionState>({ phase: 'connecting' })
  const [monitors, setMonitors] = useState<Monitor[]>([])
  const [activeMonitor, setActiveMonitor] = useState(0)
  const [quality, setQuality] = useState<QualityMode>('auto')
  const [fullscreen, setFullscreen] = useState(false)
  const [filesOpen, setFilesOpen] = useState(false)

  const { supported: kbSupported } = useKeyboardLock(fullscreen)

  useEffect(() => {
    const offState = session.onState(setState)
    const offMon = session.onMonitors((m) => setMonitors(m ?? []))
    if (canvasRef.current) session.attach(canvasRef.current)
    return () => { offState(); offMon(); session.detach() }
  }, [session])

  useEffect(() => {
    const onFs = () => setFullscreen(Boolean(document.fullscreenElement))
    document.addEventListener('fullscreenchange', onFs)
    return () => document.removeEventListener('fullscreenchange', onFs)
  }, [])

  /** Canvas is rendered with object-contain, so the drawn image is letterboxed
   *  inside the element. Mapping straight off the bounding rect would skew
   *  every click by the size of the bars. */
  const toRemote = useCallback((clientX: number, clientY: number) => {
    const c = canvasRef.current
    if (!c) return null
    const r = c.getBoundingClientRect()
    const scale = Math.min(r.width / c.width, r.height / c.height)
    const drawnW = c.width * scale
    const drawnH = c.height * scale
    const offsetX = (r.width - drawnW) / 2
    const offsetY = (r.height - drawnH) / 2
    const x = (clientX - r.left - offsetX) / scale
    const y = (clientY - r.top - offsetY) / scale
    if (x < 0 || y < 0 || x > c.width || y > c.height) return null
    return { x: Math.round(x), y: Math.round(y) }
  }, [])

  const pointer = useCallback((e: React.PointerEvent, action: 'move' | 'down' | 'up') => {
    const p = toRemote(e.clientX, e.clientY)
    if (!p) return
    session.sendPointer({ ...p, button: (e.button as 0 | 1 | 2) ?? 0, action })
  }, [session, toRemote])

  const onWheel = useCallback((e: React.WheelEvent) => {
    const p = toRemote(e.clientX, e.clientY)
    if (!p) return
    session.sendPointer({ ...p, button: 0, action: 'wheel', wheelDeltaX: e.deltaX, wheelDeltaY: e.deltaY })
  }, [session, toRemote])

  // Keyboard is bound on window, not the canvas, so it keeps working when
  // focus drifts to the toolbar. Skipped while a text input has focus so the
  // file panel's own fields still work.
  useEffect(() => {
    const typingInPanel = (t: EventTarget | null) =>
      t instanceof HTMLElement && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)

    const handle = (e: KeyboardEvent, action: 'down' | 'up') => {
      if (typingInPanel(e.target)) return
      e.preventDefault()
      session.sendKey({
        // .code, never .key - the remote host applies its own layout, so
        // sending characters breaks on any layout mismatch.
        code: e.code,
        action,
        modifiers: { alt: e.altKey, ctrl: e.ctrlKey, shift: e.shiftKey, meta: e.metaKey },
      })
    }
    const down = (e: KeyboardEvent) => handle(e, 'down')
    const up = (e: KeyboardEvent) => handle(e, 'up')
    window.addEventListener('keydown', down)
    window.addEventListener('keyup', up)
    return () => { window.removeEventListener('keydown', down); window.removeEventListener('keyup', up) }
  }, [session])

  const toggleFullscreen = () => {
    if (document.fullscreenElement) void document.exitFullscreen()
    else void shellRef.current?.requestFullscreen()
  }

  const disconnect = () => { session.disconnect(); onExit() }

  return (
    <div ref={shellRef} className="relative h-dvh w-full overflow-hidden bg-black">
      <canvas
        ref={canvasRef}
        className="session-surface size-full object-contain"
        onPointerMove={(e) => pointer(e, 'move')}
        onPointerDown={(e) => { e.currentTarget.setPointerCapture(e.pointerId); pointer(e, 'down') }}
        onPointerUp={(e) => pointer(e, 'up')}
        onWheel={onWheel}
        // Without these the browser's own menus intercept right and middle click.
        onContextMenu={(e) => e.preventDefault()}
        onAuxClick={(e) => e.preventDefault()}
      />

      <ConnectionBanner state={state} />

      <SessionToolbar
        deviceName={deviceName}
        state={state}
        monitors={monitors}
        activeMonitor={activeMonitor}
        quality={quality}
        fullscreen={fullscreen}
        filesOpen={filesOpen}
        onMonitor={(id) => { setActiveMonitor(id); session.setMonitor(id) }}
        onQuality={(q) => { setQuality(q); session.setQuality(q) }}
        onFullscreen={toggleFullscreen}
        onFiles={() => setFilesOpen((v) => !v)}
        onSAS={() => session.sendSAS()}
        onDisconnect={disconnect}
      />

      {!kbSupported && (
        <div className="absolute bottom-4 left-1/2 z-20 flex -translate-x-1/2 items-center gap-2 rounded-lg
                        bg-[hsl(var(--warning))]/95 px-4 py-2 text-xs font-medium text-black shadow-lg">
          <TriangleAlert className="size-4" />
          This browser cannot capture system shortcuts. Use Chrome or Edge for full keyboard control.
        </div>
      )}

      <FileTransferPanel session={session} open={filesOpen} onClose={() => setFilesOpen(false)} />
    </div>
  )
}
