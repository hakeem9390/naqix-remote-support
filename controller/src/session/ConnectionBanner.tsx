import { AlertTriangle, Loader2, WifiOff } from 'lucide-react'
import type { ConnectionState } from '@/mesh/types'
import { cn } from '@/lib/utils'

/** Connection state must always be legible. A frozen frame with no
 *  explanation is the single most common remote-support complaint, and the
 *  thing AnyDesk's polish mostly consists of. */
export function ConnectionBanner({ state }: { state: ConnectionState }) {
  if (state.phase === 'connected' || state.phase === 'idle') return null

  const busy = state.phase === 'connecting' || state.phase === 'authenticating' || state.phase === 'reconnecting'
  const fatal = state.phase === 'failed' || state.phase === 'disconnected'

  const text =
    state.phase === 'connecting' ? 'Connecting…'
    : state.phase === 'authenticating' ? 'Authenticating…'
    : state.phase === 'reconnecting' ? 'Connection lost — reconnecting…'
    : state.phase === 'degraded' ? 'Poor connection — reducing quality'
    : state.phase === 'disconnected' ? 'Session ended'
    : 'Connection failed'

  return (
    <div
      role="status"
      aria-live="polite"
      className={cn(
        'absolute left-1/2 top-4 z-30 flex -translate-x-1/2 items-center gap-2 rounded-full px-4 py-2',
        'text-sm font-medium shadow-lg backdrop-blur',
        fatal ? 'bg-[hsl(var(--destructive))]/90 text-white'
          : state.phase === 'degraded' ? 'bg-[hsl(var(--warning))]/90 text-black'
          : 'bg-black/70 text-white',
      )}
    >
      {busy ? <Loader2 className="size-4 animate-spin" />
        : fatal ? <WifiOff className="size-4" />
        : <AlertTriangle className="size-4" />}
      <span>{text}</span>
      {/* Never swallow the reason. */}
      {state.detail && <span className="opacity-80">— {state.detail}</span>}
    </div>
  )
}
