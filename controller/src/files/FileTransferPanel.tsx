import { useEffect, useRef, useState } from 'react'
import { Download, File, Folder, Upload, X } from 'lucide-react'
import type { SessionHandle } from '@/mesh/client'
import type { RemoteFile, TransferJob } from '@/mesh/types'
import { Button } from '@/components/ui/button'
import { cn, formatBytes, relativeTime } from '@/lib/utils'

/** showSaveFilePicker is Chromium-only. Elsewhere a download buffers the whole
 *  file in memory, so a multi-GB backup kills the tab - hence the cap. */
const HAS_FS_ACCESS = typeof window !== 'undefined' && 'showSaveFilePicker' in window
const BLOB_FALLBACK_CAP = 500 * 1024 * 1024

export function FileTransferPanel({
  session, open, onClose,
}: { session: SessionHandle; open: boolean; onClose: () => void }) {
  const [files, setFiles] = useState<RemoteFile[]>([])
  const [jobs, setJobs] = useState<TransferJob[]>([])
  const [loading, setLoading] = useState(false)
  const uploadRef = useRef<HTMLInputElement>(null)

  useEffect(() => session.onTransfer(setJobs), [session])

  useEffect(() => {
    if (!open) return
    setLoading(true)
    session.listFiles('C:\\naqix').then(setFiles).finally(() => setLoading(false))
  }, [open, session])

  const download = (f: RemoteFile) => {
    if (!HAS_FS_ACCESS && f.size > BLOB_FALLBACK_CAP) {
      alert(`${f.name} is ${formatBytes(f.size)}. This browser must buffer the whole file in memory — use Chrome or Edge for files this size.`)
      return
    }
    session.download(f.path)
  }

  return (
    <aside
      className={cn(
        'absolute right-0 top-0 z-40 flex h-full w-full max-w-md flex-col border-l border-[hsl(var(--border))]',
        'bg-[hsl(var(--card))] shadow-2xl transition-transform duration-200',
        open ? 'translate-x-0' : 'translate-x-full',
      )}
      aria-hidden={!open}
    >
      <header className="flex items-center justify-between border-b border-[hsl(var(--border))] px-4 py-3">
        <h2 className="text-sm font-semibold">Files</h2>
        <Button variant="ghost" size="icon" onClick={onClose} aria-label="Close files">
          <X />
        </Button>
      </header>

      <div className="flex items-center gap-2 border-b border-[hsl(var(--border))] px-4 py-2">
        <code className="flex-1 truncate text-xs text-[hsl(var(--muted-foreground))]">C:\naqix</code>
        <Button variant="secondary" size="sm" onClick={() => uploadRef.current?.click()}>
          <Upload /> Upload
        </Button>
        <input
          ref={uploadRef}
          type="file"
          multiple
          hidden
          onChange={(e) => {
            for (const f of Array.from(e.target.files ?? [])) session.upload(f, 'C:\\naqix')
            e.target.value = ''
          }}
        />
      </div>

      <div className="flex-1 overflow-y-auto">
        {loading && <p className="p-4 text-sm text-[hsl(var(--muted-foreground))]">Loading…</p>}
        {!loading && files.map((f) => (
          <div key={f.path} className="flex items-center gap-3 border-b border-[hsl(var(--border))]/50 px-4 py-2.5">
            {f.kind === 'directory'
              ? <Folder className="size-4 shrink-0 text-[hsl(var(--primary))]" />
              : <File className="size-4 shrink-0 text-[hsl(var(--muted-foreground))]" />}
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm">{f.name}</p>
              <p className="text-xs text-[hsl(var(--muted-foreground))]">
                {f.kind === 'file' ? formatBytes(f.size) : 'Folder'} · {relativeTime(f.modified)}
              </p>
            </div>
            {f.kind === 'file' && (
              <Button variant="ghost" size="icon" onClick={() => download(f)} aria-label={`Download ${f.name}`}>
                <Download />
              </Button>
            )}
          </div>
        ))}
      </div>

      {jobs.length > 0 && (
        <div className="max-h-56 overflow-y-auto border-t border-[hsl(var(--border))] bg-[hsl(var(--background))] p-3">
          <h3 className="mb-2 text-xs font-medium uppercase tracking-wider text-[hsl(var(--muted-foreground))]">
            Transfers
          </h3>
          <div className="space-y-2">
            {jobs.map((j) => {
              const pct = j.bytesTotal ? Math.round((j.bytesDone / j.bytesTotal) * 100) : 0
              return (
                <div key={j.id} className="text-xs">
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate">{j.direction === 'upload' ? '↑' : '↓'} {j.name}</span>
                    <span className={cn(
                      'shrink-0 tabular-nums',
                      j.state === 'error' ? 'text-[hsl(var(--destructive))]' : 'text-[hsl(var(--muted-foreground))]',
                    )}>
                      {j.state === 'done' ? 'Done' : j.state === 'error' ? 'Failed' : j.state === 'cancelled' ? 'Cancelled' : `${pct}%`}
                    </span>
                  </div>
                  <div className="mt-1 h-1 overflow-hidden rounded-full bg-[hsl(var(--secondary))]">
                    <div
                      className={cn('h-full transition-all',
                        j.state === 'error' ? 'bg-[hsl(var(--destructive))]' : 'bg-[hsl(var(--primary))]')}
                      style={{ width: `${pct}%` }}
                    />
                  </div>
                  {j.error && <p className="mt-1 text-[hsl(var(--destructive))]">{j.error}</p>}
                </div>
              )
            })}
          </div>
        </div>
      )}
    </aside>
  )
}
