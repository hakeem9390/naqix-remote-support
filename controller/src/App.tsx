import { useCallback, useEffect, useState } from 'react'
import { ShieldCheck } from 'lucide-react'
import { createMeshClient, type MeshClient, type SessionHandle } from '@/mesh/client'
import type { Device } from '@/mesh/types'
import { ConnectBar } from '@/devices/ConnectBar'
import { DeviceGrid } from '@/devices/DeviceGrid'
import { SessionView } from '@/session/SessionView'

export default function App() {
  const [client, setClient] = useState<MeshClient | null>(null)
  const [devices, setDevices] = useState<Device[]>([])
  const [loading, setLoading] = useState(true)
  const [session, setSession] = useState<{ handle: SessionHandle; name: string } | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let off: (() => void) | undefined
    createMeshClient()
      .then(async (c) => {
        setClient(c)
        setDevices(await c.listDevices())
        setLoading(false)
        off = c.subscribeDevices(setDevices)
      })
      .catch((e: Error) => { setError(e.message); setLoading(false) })
    return () => off?.()
  }, [])

  const connect = useCallback(async (deviceId: string) => {
    if (!client) return
    const device = devices.find((d) => d.id === deviceId)
    try {
      const handle = await client.connect(deviceId)
      setSession({ handle, name: device?.name ?? deviceId })
    } catch (e) {
      setError((e as Error).message)
    }
  }, [client, devices])

  // Deep link: ?device=<id> connects straight from a Naqix support ticket.
  // This is the integration no off-the-shelf tool gives us.
  useEffect(() => {
    if (!client || session) return
    const id = new URLSearchParams(window.location.search).get('device')
    if (id) void connect(id)
  }, [client, session, connect])

  if (session) {
    return (
      <SessionView
        session={session.handle}
        deviceName={session.name}
        onExit={() => setSession(null)}
      />
    )
  }

  return (
    <div className="min-h-dvh bg-[hsl(var(--background))]">
      <header className="border-b border-[hsl(var(--border))]">
        <div className="mx-auto flex max-w-6xl items-center justify-between px-6 py-4">
          <div className="flex items-center gap-2">
            <ShieldCheck className="size-5 text-[hsl(var(--primary))]" />
            <span className="font-semibold">Naqix Remote Support</span>
          </div>
          {!import.meta.env.VITE_MESH_ORIGIN && (
            <span className="rounded-full bg-[hsl(var(--warning))]/20 px-3 py-1 text-xs font-medium text-[hsl(var(--warning))]">
              Mock engine — no live MeshCentral
            </span>
          )}
        </div>
      </header>

      <main className="mx-auto max-w-6xl space-y-10 px-6 py-10">
        <ConnectBar onConnect={connect} />

        {error && (
          <div className="rounded-lg border border-[hsl(var(--destructive))]/40 bg-[hsl(var(--destructive))]/10 p-4 text-sm">
            {error}
          </div>
        )}

        <DeviceGrid devices={devices} loading={loading} onConnect={connect} />
      </main>
    </div>
  )
}
