import { Clock, MonitorSmartphone } from 'lucide-react'
import type { Device } from '@/mesh/types'
import { Button } from '@/components/ui/button'
import { StatusDot } from '@/components/ui/StatusDot'
import { cn, relativeTime } from '@/lib/utils'

export function DeviceCard({ device, onConnect }: { device: Device; onConnect: (id: string) => void }) {
  const offline = device.state === 'offline'
  const held = device.state === 'in-session'

  return (
    <div
      className={cn(
        'group flex flex-col gap-3 rounded-lg border border-[hsl(var(--border))] bg-[hsl(var(--card))] p-4 transition',
        offline ? 'opacity-60' : 'hover:border-[hsl(var(--ring))]/50 hover:shadow-lg',
      )}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <StatusDot state={device.state} />
            <h3 className="truncate font-semibold text-[hsl(var(--card-foreground))]">{device.name}</h3>
          </div>
          <p className="mt-1 truncate text-xs text-[hsl(var(--muted-foreground))]">{device.osLabel}</p>
        </div>
        <MonitorSmartphone className="size-4 shrink-0 text-[hsl(var(--muted-foreground))]" />
      </div>

      <div className="flex items-center gap-1.5 text-xs text-[hsl(var(--muted-foreground))]">
        <Clock className="size-3" />
        {offline ? `Last seen ${relativeTime(device.lastSeen)}` : held ? `Held by ${device.heldBy}` : 'Available'}
      </div>

      <Button
        variant={offline ? 'outline' : 'primary'}
        size="sm"
        disabled={offline}
        onClick={() => onConnect(device.id)}
        className="mt-auto w-full"
      >
        {offline ? 'Offline' : held ? 'Join session' : 'Connect'}
      </Button>
    </div>
  )
}
