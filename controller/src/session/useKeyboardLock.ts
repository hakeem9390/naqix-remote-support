import { useEffect, useState } from 'react'

/** navigator.keyboard.lock() is Chromium-only. Without it Cmd+W / Cmd+Q /
 *  Cmd+Tab hit the technician's own browser instead of the remote machine,
 *  which makes Safari and Firefox unusable as controllers. We do not silently
 *  degrade - the UI says so. */
export function useKeyboardLock(active: boolean) {
  const supported = typeof navigator !== 'undefined' && 'keyboard' in navigator &&
    typeof (navigator as never as { keyboard?: { lock?: unknown } }).keyboard?.lock === 'function'

  const [locked, setLocked] = useState(false)

  useEffect(() => {
    if (!supported || !active) return
    const kb = (navigator as never as { keyboard: { lock: () => Promise<void>; unlock: () => void } }).keyboard
    let cancelled = false
    // Requires a secure context and transient user activation; failure here is
    // expected rather than exceptional, so it is not surfaced as an error.
    kb.lock().then(() => { if (!cancelled) setLocked(true) }).catch(() => {})
    return () => { cancelled = true; try { kb.unlock() } catch { /* ignore */ } setLocked(false) }
  }, [supported, active])

  return { supported, locked }
}
