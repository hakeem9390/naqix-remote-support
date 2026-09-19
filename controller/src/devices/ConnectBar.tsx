import { useState } from 'react'
import { ArrowRight, Monitor } from 'lucide-react'
import { Button } from '@/components/ui/button'

/** AnyDesk's signature interaction: one big field, one button, no navigation.
 *  Supporting a known machine should never require walking a device tree. */
export function ConnectBar({ onConnect }: { onConnect: (id: string) => void }) {
  const [value, setValue] = useState('')
  const submit = (e: React.FormEvent) => {
    e.preventDefault()
    const id = value.trim()
    if (id) onConnect(id)
  }

  return (
    <form onSubmit={submit} className="w-full">
      <label
        htmlFor="device-id"
        className="mb-2 block text-xs font-medium uppercase tracking-wider text-[hsl(var(--muted-foreground))]"
      >
        Connect to device
      </label>
      <div className="flex flex-col gap-2 sm:flex-row">
        <div className="relative flex-1">
          <Monitor className="pointer-events-none absolute left-4 top-1/2 size-5 -translate-y-1/2 text-[hsl(var(--muted-foreground))]" />
          <input
            id="device-id"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder="Device ID or hostname"
            autoComplete="off"
            spellCheck={false}
            className="h-14 w-full rounded-lg border border-[hsl(var(--border))] bg-[hsl(var(--card))] pl-12 pr-4
                       text-lg tracking-wide text-[hsl(var(--card-foreground))] outline-none transition
                       placeholder:text-[hsl(var(--muted-foreground))]
                       focus:border-[hsl(var(--ring))] focus:ring-2 focus:ring-[hsl(var(--ring))]/30"
          />
        </div>
        <Button type="submit" variant="primary" size="lg" disabled={!value.trim()} className="h-14 sm:px-8">
          Connect <ArrowRight />
        </Button>
      </div>
    </form>
  )
}
