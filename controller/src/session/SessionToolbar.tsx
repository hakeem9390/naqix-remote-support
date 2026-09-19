import { useState } from 'react'
import {
  FolderOpen, Gauge, Keyboard, Maximize2, Minimize2, Monitor, PhoneOff,
} from 'lucide-react'
import type { ConnectionState, Monitor as Mon, QualityMode } from '@/mesh/types'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

const QUALITY: { value: QualityMode; label: string }[] = [
  { value: 'auto', label: 'Auto' },
  { value: 'speed', label: 'Speed' },
  { value: 'quality', label: 'Quality' },
]

export function SessionToolbar({
  deviceName, state, monitors, activeMonitor, quality, fullscreen, filesOpen,
  onMonitor, onQuality, onFullscreen, onFiles, onSAS, onDisconnect,
}: {
  deviceName: string
  state: ConnectionState
  monitors: Mon[]
  activeMonitor: number
  quality: QualityMode
  fullscreen: boolean
  filesOpen: boolean
  onMonitor: (id: number) => void
  onQuality: (q: QualityMode) => void
  onFullscreen: () => void
  onFiles: () => void
  onSAS: () => void
  onDisconnect: () => void
}) {
  const [showQuality, setShowQuality] = useState(false)

  return (
    <div
      className="absolute left-1/2 top-3 z-20 flex -translate-x-1/2 items-center gap-1 rounded-full
                 border border-white/10 bg-black/75 px-2 py-1.5 text-white shadow-2xl backdrop-blur
                 transition-opacity duration-200 hover:opacity-100 opacity-30 focus-within:opacity-100"
    >
      <span className="whitespace-nowrap px-3 text-sm font-medium">{deviceName}</span>

      {state.phase === 'connected' && (
        <span className="hidden items-center gap-3 border-l border-white/15 px-3 text-xs tabular-nums text-white/60 sm:flex">
          <span>{state.latencyMs} ms</span>
          <span>{state.fps} fps</span>
          <span>{state.kbps ? `${(state.kbps / 1000).toFixed(1)} Mbps` : ''}</span>
        </span>
      )}

      <div className="mx-1 h-6 w-px bg-white/15" />

      {monitors.length > 1 && (
        <div className="flex items-center rounded-full bg-white/10 p-0.5">
          {monitors.map((m) => (
            <button
              key={m.id}
              onClick={() => onMonitor(m.id)}
              title={`${m.label} — ${m.width}×${m.height}`}
              className={cn(
                'flex size-7 items-center justify-center rounded-full text-xs transition',
                m.id === activeMonitor ? 'bg-white text-black' : 'text-white/70 hover:text-white',
              )}
            >
              {m.id + 1}
            </button>
          ))}
        </div>
      )}

      <div className="relative">
        <Button variant="ghost" size="icon" title="Quality" onClick={() => setShowQuality((v) => !v)}
          className="text-white hover:bg-white/10">
          <Gauge />
        </Button>
        {showQuality && (
          <div className="absolute left-1/2 top-11 flex -translate-x-1/2 flex-col rounded-lg border border-white/10 bg-black/90 p-1">
            {QUALITY.map((o) => (
              <button
                key={o.value}
                onClick={() => { onQuality(o.value); setShowQuality(false) }}
                className={cn(
                  'rounded px-3 py-1.5 text-left text-xs transition hover:bg-white/10',
                  o.value === quality ? 'text-[hsl(var(--primary))]' : 'text-white/80',
                )}
              >
                {o.label}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* No browser can capture Ctrl+Alt+Del - it is the Secure Attention
          Sequence. The agent raises it via SendSAS. This button is the only way. */}
      <Button variant="ghost" size="icon" title="Send Ctrl+Alt+Del" onClick={onSAS}
        className="text-white hover:bg-white/10">
        <Keyboard />
      </Button>

      <Button variant="ghost" size="icon" title="Files" onClick={onFiles}
        className={cn('text-white hover:bg-white/10', filesOpen && 'bg-white/15')}>
        <FolderOpen />
      </Button>

      <Button variant="ghost" size="icon" title={fullscreen ? 'Exit fullscreen' : 'Fullscreen'}
        onClick={onFullscreen} className="text-white hover:bg-white/10">
        {fullscreen ? <Minimize2 /> : <Maximize2 />}
      </Button>

      <div className="mx-1 h-6 w-px bg-white/15" />

      <Button variant="ghost" size="icon" title="Disconnect" onClick={onDisconnect}
        className="text-white hover:bg-[hsl(var(--destructive))]">
        <PhoneOff />
      </Button>
    </div>
  )
}

export { Monitor as MonitorIcon }
