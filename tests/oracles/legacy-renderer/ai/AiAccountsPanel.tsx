import { useEffect, useState } from 'react'
import { Button, TextField, Message } from '../ui/primitives'
type Account = { accountId: string; email?: string; active: boolean }
type Status = { status: string; error?: string; url?: string }
export function AiAccountsPanel(): JSX.Element {
  const [accounts, setAccounts] = useState<Account[]>([])
  const [status, setStatus] = useState<Status>({ status: 'idle' })
  const [paste, setPaste] = useState('')
  const [busy, setBusy] = useState(false)
  const call = async <T,>(action: string, ...args: unknown[]): Promise<T> =>
    (await window.tezbar.aiAccounts(action, ...args)) as T
  const load = async (): Promise<void> => setAccounts(await call<Account[]>('list'))
  useEffect(() => {
    void load().catch((e) => setStatus({ status: 'error', error: String(e) }))
  }, [])
  useEffect(() => {
    if (status.status !== 'pending') return
    let stopped = false
    const timer = setInterval(() => {
      void call<Status>('status')
        .then((s) => {
          if (!stopped) {
            setStatus(s)
            if (s.status === 'success') void load()
          }
        })
        .catch((e) => {
          if (!stopped) setStatus({ status: 'error', error: String(e) })
        })
    }, 1000)
    return () => {
      stopped = true
      clearInterval(timer)
    }
  }, [status.status])
  const run = async (action: string, ...args: unknown[]): Promise<void> => {
    setBusy(true)
    try {
      const result = await call<Status>(action, ...args)
      if (result.url) {
        setStatus({ status: 'pending' })
        await window.tezbar.openExternalUrl(result.url)
      } else {
        await load()
      }
    } catch (e) {
      setStatus({ status: 'error', error: e instanceof Error ? e.message : String(e) })
    } finally {
      setBusy(false)
    }
  }
  return (
    <section className="space-y-2 text-[12px] text-ink-2" aria-label="Antigravity accounts">
      <p className="text-[11px] text-ink-3">
        This unofficial integration is not endorsed by Google. Linked accounts stay in their
        existing account store.
      </p>
      {accounts.map((a) => (
        <div className="flex items-center gap-2 py-1" key={a.accountId}>
          <span className="flex-1">{a.email || a.accountId}</span>
          {a.active ? (
            <span className="text-ink-3">Selected</span>
          ) : (
            <Button variant="quiet" disabled={busy} onClick={() => void run('switch', a.accountId)}>
              Use account
            </Button>
          )}
          <Button variant="quiet" disabled={busy} onClick={() => void run('remove', a.accountId)}>
            Remove
          </Button>
        </div>
      ))}
      <Button disabled={busy || status.status === 'pending'} onClick={() => void run('sign-in')}>
        {status.status === 'pending' ? 'Waiting for sign-in…' : 'Sign in with Google'}
      </Button>
      {status.status === 'pending' && (
        <div className="space-y-2">
          <label className="block">
            Paste the callback URL if the browser couldn't return
            <TextField
              value={paste}
              autoComplete="off"
              onChange={(e) => setPaste(e.target.value)}
            />
          </label>
          <Button onClick={() => void run('sign-in-paste', paste)}>Use callback URL</Button>
          <Button
            variant="quiet"
            onClick={() => {
              void call('cancel')
              setStatus({ status: 'cancelled' })
              setPaste('')
            }}
          >
            Cancel
          </Button>
        </div>
      )}
      {status.status === 'success' && <Message tone="success">Account linked.</Message>}
      {status.error && <Message tone="error">{status.error}</Message>}
    </section>
  )
}
