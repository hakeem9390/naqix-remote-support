import type { DeviceState } from '@/mesh/types'
import { cn } from '@/lib/utils'

const MAP: Record<DeviceState, { color: string; label: string; pulse: boolean }> = {
  online: { color: 'bg-[hsl(var(--success))]', label: 'Online', pulse: false },
  offline: { color: 'bg-[hsl(var(--muted-foreground))]', label: 'Offline', pulse: false },
  'in-session': { color: 'bg-[hsl(var(--warning))]', label: 'In session', pulse: true },
}

export function StatusDot({ state, showLabel = false }: { state: DeviceState; showLabel?: boolean }) {
  const m = MAP[state]
  return (
    <span className="inline-flex items-center gap-1.5">
      <span className="relative flex size-2">
        {m.pulse && (
          <span className={cn('absolute inline-flex size-full animate-ping rounded-full opacity-75', m.color)} />
        )}
        <span className={cn('relative inline-flex size-2 rounded-full', m.color)} />
      </span>
      {showLabel && <span className="text-xs text-[hsl(var(--muted-foreground))]">{m.label}</span>}
    </span>
  )
}
