import { useMemo, useState } from 'react'
import { Search } from 'lucide-react'
import type { Device } from '@/mesh/types'
import { DeviceCard } from './DeviceCard'
import { Badge } from '@/components/ui/badge'

/** Grouped by ERP tenant, not by hostname. A technician picking up a ticket
 *  thinks "Acme Pharmacy", never "RECEPTION-PC". */
export function DeviceGrid({
  devices, loading, onConnect,
}: { devices: Device[]; loading: boolean; onConnect: (id: string) => void }) {
  const [q, setQ] = useState('')

  const groups = useMemo(() => {
    const needle = q.trim().toLowerCase()
    const filtered = needle
      ? devices.filter((d) =>
          d.name.toLowerCase().includes(needle) ||
          d.tenant.toLowerCase().includes(needle) ||
          d.id.toLowerCase().includes(needle))
      : devices

    const byTenant = new Map<string, Device[]>()
    for (const d of filtered) {
      const list = byTenant.get(d.tenant) ?? []
      list.push(d)
      byTenant.set(d.tenant, list)
    }
    // Tenants with someone online first - that is where the work is.
    return [...byTenant.entries()]
      .map(([tenant, list]) => ({
        tenant,
        devices: list.sort((a, b) => Number(b.state !== 'offline') - Number(a.state !== 'offline')),
        onlineCount: list.filter((d) => d.state !== 'offline').length,
      }))
      .sort((a, b) => b.onlineCount - a.onlineCount || a.tenant.localeCompare(b.tenant))
  }, [devices, q])

  if (loading) return <GridSkeleton />

  return (
    <div className="space-y-8">
      <div className="relative max-w-sm">
        <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-[hsl(var(--muted-foreground))]" />
        <input
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Filter machines or customers"
          className="h-9 w-full rounded-md border border-[hsl(var(--border))] bg-[hsl(var(--card))] pl-9 pr-3 text-sm
                     outline-none focus:border-[hsl(var(--ring))]"
        />
      </div>

      {groups.length === 0 && (
        <p className="py-12 text-center text-sm text-[hsl(var(--muted-foreground))]">
          No machines match “{q}”.
        </p>
      )}

      {groups.map(({ tenant, devices: list, onlineCount }) => (
        <section key={tenant}>
          <div className="mb-3 flex items-center gap-2">
            <h2 className="text-sm font-semibold text-[hsl(var(--foreground))]">{tenant}</h2>
            <Badge>{onlineCount} of {list.length} online</Badge>
          </div>
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
            {list.map((d) => <DeviceCard key={d.id} device={d} onConnect={onConnect} />)}
          </div>
        </section>
      ))}
    </div>
  )
}

function GridSkeleton() {
  return (
    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
      {Array.from({ length: 6 }).map((_, i) => (
        <div key={i} className="h-36 animate-pulse rounded-lg border border-[hsl(var(--border))] bg-[hsl(var(--card))]" />
      ))}
    </div>
  )
}
