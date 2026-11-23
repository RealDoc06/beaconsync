// One server: live console with a command line, start/stop/restart, and its backups
// (BeaconSync's own and backup mods' like FTB Backups). Everything goes through the hub,
// so it works wherever the server runs.

import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import {
  ArchiveIcon,
  ChevronLeftIcon,
  PlayIcon,
  RefreshIcon,
  StopIcon,
  TerminalIcon,
  TrashIcon,
  UndoIcon,
} from '../../components/Icons'
import { Button, Empty, Input, Tabs, Tag, type TagTone } from '../../components/ui'
import { call } from '../../lib/api'
import { ago } from '../../lib/format'
import { askConfirm, useApp } from '../../lib/store'
import type { Backup, ConsoleView, Retention, Server, ServerAction } from '../../lib/types'

/** Lines kept in memory. */
const KEEP = 5000

type Live = Omit<ConsoleView, 'next'> & { loaded: boolean; error: string | null }

/** Long polls the server's console through the hub while mounted. */
function useConsole(path: string, server: string): Live {
  const [live, setLive] = useState<Live>({
    lines: [],
    backups: [],
    online: false,
    running: false,
    retention: null,
    loaded: false,
    error: null,
  })
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

export function ServerPanel({
  path,
  server,
  version,
  latest,
  onBack,
}: {
  path: string
  server: Server
  version: string
  /** Newer beacon this server can update to. */
  latest: string | null
  onBack: () => void
}) {
  const live = useConsole(path, server.id)
  const [tab, setTab] = useState<'console' | 'backups'>('console')
  const set = useApp((s) => s.set)
  /** Queues an action on the server; false (after showing why) when the hub refused it. */
  const send: Send = async (action, done) => {
    try {
      await call('server_action', { path, server: server.id, action })
      if (done) set({ notice: { kind: 'ok', text: done } })
      return true
    } catch (e) {
      set({ notice: { kind: 'error', text: e instanceof Error ? e.message : String(e) } })
      return false
    }
  }
  const state = !live.loaded ? 'connecting' : !live.online ? 'offline' : live.running ? 'running' : 'stopped'
  const stateTones = { connecting: 'muted', offline: 'muted', running: 'ok', stopped: 'warn' } as const satisfies Record<
    typeof state,
    TagTone
  >
  const tone: TagTone = stateTones[state]

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex flex-wrap items-center gap-2.5 border-b border-line bg-sidebar/40 px-6 py-3">
        <Button variant="ghost" small onClick={onBack}>
          <ChevronLeftIcon className="size-3.5" />
          Servers
        </Button>
        <span className="h-4 border-l border-line-strong" />
        <span className="font-semibold text-fg">{server.name}</span>
        <Tag tone={tone} dot>
          {state}
        </Tag>
        <span className="min-w-0 truncate text-xs text-muted">
          {version} · <span className="font-mono">{server.address}</span> · beacon {server.agent_version ?? 'unknown'}
        </span>
        <span className="flex-1" />
        {latest && (
          <Button
            small
            disabled={!live.online}
            onClick={() =>
              askConfirm({
                title: `Update beacon to ${latest}?`,
                body: 'The server downloads and checks the new beacon, then warns players and restarts Minecraft on it. Takes about a minute.',
                action: 'Update',
                onConfirm: () => void send({ kind: 'update', arg: latest }, 'Updating beacon. Follow along in the console.'),
              })
            }
          >
            <RefreshIcon className="size-3.5" />
            Update beacon
          </Button>
        )}
        {live.running ? (
          <Button
            small
            variant="danger"
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
            <StopIcon className="size-3" />
            Stop
          </Button>
        ) : (
          <Button small variant="primary" disabled={!live.online} onClick={() => void send({ kind: 'start' })}>
            <PlayIcon className="size-3" />
            Start
          </Button>
        )}
        <Button
          small
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
          <RefreshIcon className="size-3.5" />
          Restart
        </Button>
      </div>

      <Tabs
        value={tab}
        onChange={setTab}
        tabs={[
          { value: 'console', label: 'Console', icon: <TerminalIcon className="size-3.5" /> },
          {
            value: 'backups',
            icon: <ArchiveIcon className="size-3.5" />,
            label: (
              <span className="flex items-center gap-1.5">
                Backups
                <span className="rounded-full bg-surface-2 px-1.5 py-px font-mono text-[10.5px] text-muted tabular-nums">
                  {live.backups.length}
                </span>
              </span>
            ),
          },
        ]}
      />

      {live.error && (
        <div className="border-b border-bad-border bg-bad-soft px-6 py-1.5 text-xs text-bad">{live.error}</div>
      )}

      {tab === 'console' ? (
        <Console lines={live.lines} disabled={!live.running} onSend={(c) => void send({ kind: 'command', arg: c })} />
      ) : (
        <Backups backups={live.backups} retention={live.retention} online={live.online} send={send} />
      )}
    </div>
  )
}

function lineTone(l: string) {
  if (l.startsWith('[beacon]')) return 'text-accent-text'
  if (l.startsWith('> ')) return 'text-fg font-semibold'
  if (l.includes('/ERROR]') || l.includes(' ERROR ')) return 'text-bad'
  if (l.includes('/WARN]') || l.includes(' WARN ')) return 'text-warn'
  return 'text-fg-secondary'
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
    <div className="flex min-h-0 flex-1 flex-col bg-console">
      <div
        ref={box}
        onScroll={(e) => {
          const el = e.currentTarget
          follow.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24
        }}
        className="min-h-0 flex-1 overflow-auto px-6 py-3 font-mono text-xs leading-5 select-text"
      >
        {lines.length === 0 && <div className="text-subtle">Waiting for server output…</div>}
        {lines.map((l, i) => (
          <div key={i} className={`whitespace-pre-wrap break-all ${lineTone(l)}`}>
            {l}
          </div>
        ))}
      </div>
      <form
        className="flex items-center gap-2.5 border-t border-line bg-surface px-6 py-2.5 focus-within:border-accent"
        onSubmit={(e) => {
          e.preventDefault()
          submit()
        }}
      >
        <span className="font-mono text-xs font-semibold text-accent-text">&gt;</span>
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
          className="h-8 min-w-0 flex-1 rounded-md bg-transparent px-2 font-mono text-xs text-fg outline-none placeholder:text-subtle disabled:opacity-40"
        />
        <Button small variant="primary" type="submit" disabled={disabled || !input.trim()}>
          Send
        </Button>
      </form>
    </div>
  )
}

type Send = (a: ServerAction, done?: string) => Promise<boolean>

const GB = 2 ** 30
const DAY = 86_400
const COLS =
  'grid-cols-[18px_minmax(100px,135px)_105px_70px_102px_minmax(0,1fr)_auto] @max-[760px]:grid-cols-[18px_minmax(0,1fr)_96px_64px_auto]'

const total = (bs: Backup[]) => bs.reduce((n, b) => n + b.size, 0)

function Backups({
  backups,
  retention,
  online,
  send,
}: {
  backups: Backup[]
  retention: Retention | null
  online: boolean
  send: Send
}) {
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set())
  // Ids can go stale as backups are pruned or deleted; only listed ones count.
  const picked = backups.filter((b) => selected.has(b.id))
  const all = backups.length > 0 && picked.length === backups.length
  const flip = (id: string) =>
    setSelected((s) => {
      const next = new Set(s)
      if (!next.delete(id)) next.add(id)
      return next
    })

  const restore = (b: Backup) =>
    askConfirm({
      title: 'Restore this backup?',
      body: `${b.full ? 'The whole server' : 'The world'} goes back to ${ago(b.created)} (${b.source}). Minecraft restarts and changes since then are lost. A safety backup is taken first.`,
      action: 'Restore',
      danger: true,
      onConfirm: () => void send({ kind: 'restore', arg: b.id }, 'Restoring. Follow along in the console.'),
    })
  const remove = (doomed: Backup[]) => {
    const [one] = doomed
    askConfirm({
      title: doomed.length === 1 ? 'Delete this backup?' : `Delete ${doomed.length} backups?`,
      body: `${doomed.length === 1 && one ? `${one.source} backup from ${ago(one.created)} (${size(one.size)})` : `${size(total(doomed))} in total`}. This can't be undone.`,
      action: 'Delete',
      danger: true,
      onConfirm: async () => {
        // One action per backup: the server works through them in order. Stops at the first
        // refusal (e.g. the server went offline), leaving the rest selected.
        for (const b of doomed) {
          if (!(await send({ kind: 'delete_backup', arg: b.id }))) return
          setSelected((s) => new Set([...s].filter((id) => id !== b.id)))
        }
      },
    })
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto p-6">
      <div className="flex flex-wrap items-center gap-3">
        <Button variant="primary" small disabled={!online} onClick={() => void send({ kind: 'backup' }, 'Backing up. Follow along in the console.')}>
          <ArchiveIcon className="size-3.5" />
          Back up now
        </Button>
        <span className="text-xs text-muted">
          Backups are also taken before every update. Backups from mods like FTB Backups appear here too.
        </span>
      </div>

      {retention && (
        <RetentionRow
          key={JSON.stringify(retention)}
          retention={retention}
          own={backups.filter((b) => b.full)}
          online={online}
          send={send}
        />
      )}

      <div className="@container shrink-0 overflow-x-auto rounded-2xl border border-line bg-surface shadow-theme-xs">
        {backups.length === 0 ? (
          <Empty icon={<ArchiveIcon className="size-5" />} title={online ? 'No backups yet' : 'Server is offline'}>
            {online ? 'Create a manual backup above or deploy an update.' : 'Backups will load once the server reconnects.'}
          </Empty>
        ) : (
          <>
            <div className="flex h-10 items-center gap-3 border-b border-line bg-sidebar/60 px-4 text-xs text-muted">
              <input
                type="checkbox"
                aria-label="Select all backups"
                checked={all}
                ref={(el) => {
                  if (el) el.indeterminate = picked.length > 0 && !all
                }}
                onChange={() => setSelected(new Set(all ? [] : backups.map((b) => b.id)))}
                className="accent-accent"
              />
              <span className="min-w-0 flex-1 truncate font-medium text-fg-secondary">
                {picked.length
                  ? `${picked.length} selected · ${size(total(picked))}`
                  : `${backups.length} backup${backups.length === 1 ? '' : 's'} · ${size(total(backups))}`}
              </span>
              <span className="flex shrink-0 justify-end gap-1.5">
                {picked.length > 0 && (
                  <>
                    <Button small variant="ghost" onClick={() => setSelected(new Set())}>
                      Clear
                    </Button>
                    <Button small variant="danger" disabled={!online} onClick={() => remove(picked)}>
                      <TrashIcon className="size-3" />
                      Delete {picked.length}
                    </Button>
                  </>
                )}
              </span>
            </div>
            <div className="divide-y divide-line">
              {backups.map((b) => (
                <div
                  key={b.id}
                  className={`grid min-h-12 ${COLS} items-center gap-3 px-4 py-1.5 transition-colors hover:bg-hover ${
                    selected.has(b.id) ? 'bg-active' : ''
                  }`}
                >
                  <input
                    type="checkbox"
                    aria-label={`Select ${b.source} backup from ${ago(b.created)}`}
                    checked={selected.has(b.id)}
                    onChange={() => flip(b.id)}
                    className="accent-accent"
                  />
                  <div className="min-w-0">
                    <div className="truncate font-medium text-fg">{b.source}</div>
                    <div className="mt-0.5 hidden min-w-0 items-center gap-1.5 text-[11px] text-muted @max-[760px]:flex">
                      <span className="shrink-0 font-medium text-fg-secondary">{b.full ? 'Whole server' : 'World'}</span>
                      {b.note && (
                        <>
                          <span className="text-subtle">·</span>
                          <span className="truncate" title={b.note}>
                            {b.note}
                          </span>
                        </>
                      )}
                    </div>
                  </div>
                  <span className="truncate text-xs text-muted" title={new Date(b.created * 1000).toLocaleString()}>
                    {ago(b.created)}
                  </span>
                  <span className="text-right font-mono text-xs text-muted tabular-nums">{size(b.size)}</span>
                  <div className="@max-[760px]:hidden">
                    <Tag tone={b.full ? 'accent' : 'muted'}>{b.full ? 'Whole server' : 'World'}</Tag>
                  </div>
                  <span className="truncate text-xs text-muted @max-[760px]:hidden" title={b.id}>
                    {b.note}
                  </span>
                  <span className="flex shrink-0 justify-end gap-1.5">
                    <Button small disabled={!online} onClick={() => restore(b)}>
                      <UndoIcon className="size-3" />
                      Restore
                    </Button>
                    <Button small variant="danger" disabled={!online} onClick={() => remove([b])}>
                      Delete
                    </Button>
                  </span>
                </div>
              ))}
            </div>
          </>
        )}
      </div>
    </div>
  )
}

/** Parses one limit field: null when empty (no limit), undefined when invalid. Counts and days
 *  are whole numbers that fit a u32; sizes are GB and become bytes. */
function limit(text: string, unit: 'count' | 'gb'): number | null | undefined {
  if (!text.trim()) return null
  const n = Number(text)
  if (unit === 'gb') return n > 0 && n * GB <= Number.MAX_SAFE_INTEGER ? Math.max(1, Math.round(n * GB)) : undefined
  return Number.isInteger(n) && n >= 1 && n <= 2 ** 32 - 1 ? n : undefined
}

/** How many of `own` (BeaconSync backups, newest first) `r` keeps. Mirrors `kept` in crates/cli/src/backups.rs. */
function kept(own: Backup[], r: Retention, now: number) {
  let bytes = 0
  const first = own.findIndex((b, i) => {
    bytes += b.size
    return (
      i > 0 &&
      ((r.keep !== null && i >= r.keep) ||
        (r.max_bytes !== null && bytes > r.max_bytes) ||
        (r.max_days !== null && now - b.created > r.max_days * DAY))
    )
  })
  return first < 0 ? own.length : first
}

const draftOf = (r: Retention) => ({
  keep: r.keep?.toString() ?? '',
  gb: r.max_bytes === null ? '' : String(Math.round((r.max_bytes / GB) * 100) / 100),
  days: r.max_days?.toString() ?? '',
})

/** The server's retention rule as three editable limits. Remounted (via key) when the server reports a new one. */
function RetentionRow({ retention, own, online, send }: { retention: Retention; own: Backup[]; online: boolean; send: Send }) {
  const saved = draftOf(retention)
  const [draft, setDraft] = useState(saved)
  // An untouched size keeps its exact bytes; the field only shows it rounded.
  const parsed = {
    keep: limit(draft.keep, 'count'),
    max_bytes: draft.gb === saved.gb ? retention.max_bytes : limit(draft.gb, 'gb'),
    max_days: limit(draft.days, 'count'),
  }
  const { keep, max_bytes, max_days } = parsed
  const next: Retention | null =
    keep !== undefined && max_bytes !== undefined && max_days !== undefined ? { keep, max_bytes, max_days } : null
  const dirty = draft.keep !== saved.keep || draft.gb !== saved.gb || draft.days !== saved.days
  const save = (r: Retention) => {
    const doomed = own.slice(kept(own, r, Date.now() / 1000))
    const go = () => void send({ kind: 'set_retention', arg: r }, 'Saved')
    if (!doomed.length) return go()
    askConfirm({
      title: `Delete ${doomed.length} backup${doomed.length === 1 ? '' : 's'} now?`,
      body: `They are past the new limits (${size(total(doomed))}). This can't be undone.`,
      action: 'Save and delete',
      danger: true,
      onConfirm: go,
    })
  }
  const field = (key: keyof typeof draft, value: keyof typeof parsed, label: string, unit: string) => (
    <label className="flex items-center gap-1.5">
      <span>{label}</span>
      <Input
        inputMode="decimal"
        inputSize="sm"
        value={draft[key]}
        placeholder="any"
        onChange={(e) => setDraft((d) => ({ ...d, [key]: e.target.value }))}
        className={`w-15 text-right font-mono ${parsed[value] === undefined ? 'border-bad!' : ''}`}
      />
      <span>{unit}</span>
    </label>
  )
  return (
    <form
      className="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-xl border border-line bg-surface px-4 py-3 text-xs text-muted shadow-theme-xs"
      onSubmit={(e) => {
        e.preventDefault()
        if (next && dirty) save(next)
      }}
    >
      <span className="font-semibold text-fg">Keep</span>
      {field('keep', 'keep', 'last', 'backups')}
      {field('gb', 'max_bytes', 'up to', 'GB')}
      {field('days', 'max_days', 'up to', 'days old')}
      <Button small variant={dirty ? 'primary' : 'default'} type="submit" disabled={!online || !dirty || !next}>
        Save
      </Button>
      <span className="text-subtle">Oldest BeaconSync backups go first; the newest always stays.</span>
    </form>
  )
}
