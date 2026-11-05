// One server: live console with a command line, start/stop/restart, and its backups
// (BeaconSync's own and backup mods' like FTB Backups). Everything goes through the hub,
// so it works wherever the server runs.

import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { Button, Empty, Tabs } from '../../components/ui'
import { call } from '../../lib/api'
import { ago } from '../../lib/format'
import { askConfirm, useApp } from '../../lib/store'
import type { Backup, ConsoleView, Server, ServerAction } from '../../lib/types'

/** Lines kept in memory. */
const KEEP = 5000

type Live = Omit<ConsoleView, 'next'> & { loaded: boolean; error: string | null }

/** Long polls the server's console through the hub while mounted. */
function useConsole(path: string, server: string): Live {
  const [live, setLive] = useState<Live>({ lines: [], backups: [], online: false, running: false, loaded: false, error: null })
  useEffect(() => {
    let alive = true
    let after: number | null = null
    void (async () => {
      while (alive) {
        try {
          const v: ConsoleView = await call('server_console', { path, server, after })
          if (!alive) return
          after = v.next
          setLive((s) => ({ ...v, lines: [...s.lines, ...v.lines].slice(-KEEP), loaded: true, error: null }))
        } catch (e) {
          if (!alive) return
          setLive((s) => ({ ...s, error: e instanceof Error ? e.message : String(e) }))
          await new Promise((r) => setTimeout(r, 3000))
        }
      }
    })()
    return () => {
      alive = false
    }
  }, [path, server])
  return live
}

function size(bytes: number) {
  if (bytes < 1 << 20) return `${Math.ceil(bytes / 1024)} KB`
  if (bytes < 1 << 30) return `${Math.round(bytes / (1 << 20))} MB`
  return `${(bytes / (1 << 30)).toFixed(1)} GB`
}

export function ServerPanel({ path, server, version, onBack }: { path: string; server: Server; version: string; onBack: () => void }) {
  const live = useConsole(path, server.id)
  const [tab, setTab] = useState<'console' | 'backups'>('console')
  const set = useApp((s) => s.set)
  const send = async (action: ServerAction, done?: string) => {
    try {
      await call('server_action', { path, server: server.id, action })
      if (done) set({ notice: { kind: 'ok', text: done } })
    } catch (e) {
      set({ notice: { kind: 'error', text: e instanceof Error ? e.message : String(e) } })
    }
  }
  const state = !live.loaded ? 'connecting' : !live.online ? 'offline' : live.running ? 'running' : 'stopped'
  const tone = { connecting: 'text-muted', offline: 'text-[#444]', running: 'text-ok', stopped: 'text-warn' }[state]

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-3 px-5 py-3">
        <Button variant="ghost" small onClick={onBack}>
          ‹ Servers
        </Button>
        <span className={tone}>●</span>
        <span className="font-semibold">{server.name}</span>
        <span className="text-xs text-muted">
          {state} · {version} · <span className="font-mono">{server.address}</span>
        </span>
        <span className="flex-1" />
        {live.running ? (
          <Button
            disabled={!live.online}
            onClick={() =>
              askConfirm({
                title: `Stop ${server.name}?`,
                body: 'Minecraft stops and players are disconnected. It stays stopped until you start it again.',
                action: 'Stop',
                danger: true,
                onConfirm: () => void send({ kind: 'stop' }),
              })
            }
          >
            Stop
          </Button>
        ) : (
          <Button disabled={!live.online} onClick={() => void send({ kind: 'start' })}>
            Start
          </Button>
        )}
        <Button
          disabled={!live.online}
          onClick={() =>
            askConfirm({
              title: `Restart ${server.name}?`,
              body: 'Players are disconnected while Minecraft restarts.',
              action: 'Restart',
              onConfirm: () => void send({ kind: 'restart' }),
            })
          }
        >
          Restart
        </Button>
      </div>
      <Tabs
        value={tab}
        onChange={setTab}
        tabs={[
          { value: 'console', label: 'Console' },
          { value: 'backups', label: <>Backups <span className="text-muted">{live.backups.length}</span></> },
        ]}
      />
      {live.error && <div className="px-5 py-1 text-xs text-bad">{live.error}</div>}
      {tab === 'console' ? (
        <Console lines={live.lines} disabled={!live.running} onSend={(c) => void send({ kind: 'command', arg: c })} />
      ) : (
        <Backups backups={live.backups} online={live.online} send={send} />
      )}
    </div>
  )
}

function lineTone(l: string) {
  if (l.startsWith('[beacon]')) return 'text-accent'
  if (l.startsWith('> ')) return 'text-fg font-semibold'
  if (l.includes('/ERROR]') || l.includes(' ERROR ')) return 'text-bad'
  if (l.includes('/WARN]') || l.includes(' WARN ')) return 'text-warn'
  return 'text-[#cfcfcf]'
}

function Console({ lines, disabled, onSend }: { lines: string[]; disabled: boolean; onSend: (command: string) => void }) {
  const box = useRef<HTMLDivElement>(null)
  // Follow new output unless the user scrolled up to read.
  const follow = useRef(true)
  const [input, setInput] = useState('')
  const history = useRef<string[]>([])
  const [hist, setHist] = useState<number | null>(null)
  useLayoutEffect(() => {
    if (follow.current && box.current) box.current.scrollTop = box.current.scrollHeight
  }, [lines])

  const submit = () => {
    const c = input.trim().replace(/^\//, '')
    if (!c) return
    if (history.current.at(-1) !== c) history.current.push(c)
    setInput('')
    setHist(null)
    follow.current = true
    onSend(c)
  }
  const browse = (dir: -1 | 1) => {
    const h = history.current
    if (!h.length) return
    const next = hist === null ? (dir < 0 ? h.length - 1 : null) : hist + dir >= h.length ? null : Math.max(0, hist + dir)
    setHist(next)
    setInput(next === null ? '' : (h[next] ?? ''))
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div
        ref={box}
        onScroll={(e) => {
          const el = e.currentTarget
          follow.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24
        }}
        className="min-h-0 flex-1 overflow-auto px-5 py-2 font-mono text-xs leading-5 select-text"
      >
        {lines.length === 0 && <div className="text-muted">Waiting for output…</div>}
        {lines.map((l, i) => (
          <div key={i} className={`whitespace-pre-wrap break-all ${lineTone(l)}`}>
            {l}
          </div>
        ))}
      </div>
      <form
        className="flex items-center gap-2 border-t border-line px-5 py-2"
        onSubmit={(e) => {
          e.preventDefault()
          submit()
        }}
      >
        <span className="font-mono text-muted">&gt;</span>
        <input
          value={input}
          disabled={disabled}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return
            e.preventDefault()
            browse(e.key === 'ArrowUp' ? -1 : 1)
          }}
          placeholder={disabled ? 'Minecraft is not running' : 'Type a command, e.g. list or whitelist add Steve'}
          className="h-8 min-w-0 flex-1 bg-transparent font-mono text-xs outline-none placeholder:text-[#555] disabled:opacity-40"
        />
        <Button small type="submit" disabled={disabled || !input.trim()}>
          Send
        </Button>
      </form>
    </div>
  )
}

function Backups({ backups, online, send }: { backups: Backup[]; online: boolean; send: (a: ServerAction, done?: string) => Promise<void> }) {
  const restore = (b: Backup) =>
    askConfirm({
      title: 'Restore this backup?',
      body: `${b.full ? 'The whole server' : 'The world'} goes back to ${ago(b.created)} (${b.source}). Minecraft restarts and changes since then are lost. A safety backup is taken first.`,
      action: 'Restore',
      danger: true,
      onConfirm: () => void send({ kind: 'restore', arg: b.id }, 'Restoring. Follow along in the console.'),
    })
  const remove = (b: Backup) =>
    askConfirm({
      title: 'Delete this backup?',
      body: `${b.source} backup from ${ago(b.created)} (${size(b.size)}). This can't be undone.`,
      action: 'Delete',
      danger: true,
      onConfirm: () => void send({ kind: 'delete_backup', arg: b.id }),
    })
  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-auto">
      <div className="flex items-center gap-2 px-5 py-3">
        <Button variant="primary" disabled={!online} onClick={() => void send({ kind: 'backup' }, 'Backing up. Follow along in the console.')}>
          Back up now
        </Button>
        <span className="text-xs text-muted">
          Backups are also taken before every update. Backups made by mods like FTB Backups show up here too.
        </span>
      </div>
      <div className="border-t border-line">
        {backups.length === 0 && <Empty>{online ? 'No backups yet.' : 'The server is offline.'}</Empty>}
        {backups.map((b) => (
          <div key={b.id} className="grid h-11 grid-cols-[140px_120px_80px_110px_minmax(0,1fr)_auto] items-center gap-3 border-b border-line px-5">
            <span className="truncate font-medium">{b.source}</span>
            <span className="text-muted" title={new Date(b.created * 1000).toLocaleString()}>
              {ago(b.created)}
            </span>
            <span className="text-right text-muted">{size(b.size)}</span>
            <span className="text-xs text-muted">{b.full ? 'whole server' : 'world'}</span>
            <span className="truncate text-xs text-muted" title={b.id}>
              {b.note}
            </span>
            <span className="flex gap-1.5">
              <Button small disabled={!online} onClick={() => restore(b)}>
                Restore
              </Button>
              <Button small variant="danger" disabled={!online} onClick={() => remove(b)}>
                Delete
              </Button>
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}
