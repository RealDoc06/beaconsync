import { useQuery, useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { useEffect, type ReactNode } from 'react'
import { Conflicts } from './components/Conflicts'
import {
  AlertIcon,
  CheckIcon,
  CloseIcon,
  LibraryIcon,
  LinkCodeIcon,
  MonitorIcon,
  MoonIcon,
  SettingsIcon,
  SunIcon,
  UserIcon,
} from './components/Icons'
import { InstanceIcon } from './components/InstanceIcon'
import { Button, Dialog, ProgressBar, Select } from './components/ui'
import { UpdateButton, useUpdate } from './components/UpdateButton'
import { call } from './lib/api'
import { askConfirm, keys, useAction, useApp, type View } from './lib/store'
import { useTheme, type ThemeMode } from './lib/theme'
import type { GameFailure, Progress, UpdateView } from './lib/types'
import { ImportView } from './views/Import'
import { JoinView } from './views/Join'
import { LibraryView } from './views/Library'
import { NewPack } from './views/NewPack'
import { PackView } from './views/Pack'
import { SettingsView } from './views/Settings'

export function App() {
  const view = useApp((s) => s.view)
  const set = useApp((s) => s.set)
  const qc = useQueryClient()

  useEffect(() => {
    const offs = [
      listen<Progress>('progress', (e) => set({ progress: e.payload })),
      listen<UpdateView>('update', (e) => qc.setQueryData(keys.update, e.payload)),
      listen<GameFailure>('game-failed', ({ payload: f }) =>
        set({
          notice:
            f.kind === 'crashed'
              ? { kind: 'error', text: 'Minecraft crashed', open: { path: f.report, label: 'Open crash report' } }
              : { kind: 'error', text: 'Minecraft could not start', open: f.log ? { path: f.log, label: 'Open log' } : undefined },
        }),
      ),
    ]
    return () => offs.forEach((off) => void off.then((f) => f()))
  }, [set, qc])

  return (
    <div className="flex h-full flex-col bg-bg text-fg">
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <main className="flex min-w-0 flex-1 flex-col bg-bg">
          <Page view={view} />
        </main>
      </div>
      <StatusBar />
      <NewPack />
      <ConfirmDialog />
      <Conflicts />
    </div>
  )
}

function Page({ view }: { view: View }) {
  switch (view.name) {
    case 'library':
      return <LibraryView />
    case 'import':
      return <ImportView />
    case 'join':
      return <JoinView initial={view.code} />
    case 'settings':
      return <SettingsView />
    case 'pack':
      return <PackView key={view.path} path={view.path} tab={view.tab} />
  }
}

function Sidebar() {
  const view = useApp((s) => s.view)
  const go = useApp((s) => s.go)
  const library = useQuery({ queryKey: keys.library, queryFn: () => call('list_instances') })
  const settings = useQuery({ queryKey: keys.settings, queryFn: () => call('get_settings') })
  const version = useUpdate()?.current
  const tracked = library.data?.filter((i) => i.tracked) ?? []

  const nav = (name: 'library' | 'join' | 'settings', label: string, icon: ReactNode) => {
    const active = view.name === name || (name === 'library' && view.name === 'import')
    return (
      <button
        type="button"
        onClick={() => go({ name })}
        className={`group flex h-8.5 items-center gap-2.5 rounded-lg px-2.5 text-left text-[13px] font-medium transition-colors ${
          active ? 'bg-active text-fg ring-1 ring-accent-border' : 'text-muted hover:bg-hover hover:text-fg'
        }`}
      >
        <span className={active ? 'text-accent-text' : 'text-muted transition-colors group-hover:text-fg-secondary'}>
          {icon}
        </span>
        <span className="truncate">{label}</span>
      </button>
    )
  }

  return (
    <aside className="flex w-60 shrink-0 flex-col border-r border-line bg-sidebar">
      <div
        data-tauri-drag-region
        className={`flex shrink-0 items-center gap-1.5 ${isMac ? 'h-9 pl-[76px] pr-3' : 'h-11 px-3.5'}`}
      >
        <span className="shrink-0 text-[13px] font-semibold leading-none tracking-tight text-fg">BeaconSync</span>
        {version && (
          <span className="min-w-0 truncate font-mono text-[10.5px] leading-none text-subtle" title={version}>
            v{version.replace(/^v/, '')}
          </span>
        )}
        <UpdateButton />
      </div>

      <nav className="flex flex-col gap-0.5 px-2.5 pt-1.5">
        {nav('library', 'Library', <LibraryIcon className="size-4" />)}
        {nav('join', 'Join with code', <LinkCodeIcon className="size-4" />)}
        {nav('settings', 'Settings', <SettingsIcon className="size-4" />)}
      </nav>

      <div className="mx-3.5 my-3 border-t border-line" />

      <div className="flex items-center justify-between px-3.5 pb-1.5">
        <span className="text-[10.5px] font-semibold uppercase tracking-[0.06em] text-subtle">Tracked packs</span>
        {tracked.length > 0 && (
          <span className="rounded-full bg-surface-2 px-1.5 py-px font-mono text-[10px] font-medium text-muted tabular-nums">
            {tracked.length}
          </span>
        )}
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-auto px-2.5 pb-2">
        {tracked.length === 0 ? (
          <div className="px-1.5 py-2 text-xs text-subtle">No tracked packs yet</div>
        ) : (
          tracked.map((i) => {
            const active = view.name === 'pack' && view.path === i.path
            return (
              <button
                key={i.path}
                type="button"
                onClick={() => go({ name: 'pack', path: i.path, tab: 'mods' })}
                className={`flex h-8.5 items-center gap-2 rounded-lg px-2 text-left transition-colors ${
                  active
                    ? 'bg-surface text-fg shadow-theme-xs ring-1 ring-line-strong'
                    : 'text-fg-secondary hover:bg-hover hover:text-fg'
                }`}
              >
                <InstanceIcon path={i.path} size={20} framed={false} />
                <span className="min-w-0 flex-1 truncate text-[12.5px] font-medium">{i.name}</span>
                <span
                  className={`shrink-0 rounded px-1.5 py-0.5 font-mono text-[10px] leading-none ${
                    i.role === 'subscriber'
                      ? 'bg-info-soft text-info'
                      : active
                        ? 'bg-accent-soft text-accent-text'
                        : 'bg-surface-2/80 text-muted'
                  }`}
                >
                  {i.role === 'subscriber' ? 'joined' : i.minecraft}
                </span>
              </button>
            )
          })
        )}
      </div>

      <div className="flex flex-col border-t border-line bg-sidebar/80">
        <MinecraftAccount />
        <div className="flex items-center justify-between gap-2 border-t border-line px-3 py-2.5">
          {settings.data?.signed_in ? (
            <button
              type="button"
              onClick={() => go({ name: 'settings' })}
              className="flex min-w-0 items-center gap-2 rounded-lg px-1.5 py-1 text-left transition-colors hover:bg-hover"
              title={`Signed in as ${settings.data.username}`}
            >
              <span className="grid size-5.5 shrink-0 place-items-center rounded-full bg-accent-soft text-[11px] font-semibold uppercase text-accent-text ring-1 ring-accent-border">
                {(settings.data.username ?? '?').slice(0, 1)}
              </span>
              <span className="truncate text-xs font-medium text-fg">{settings.data.username}</span>
            </button>
          ) : (
            <button
              type="button"
              onClick={() => go({ name: 'settings' })}
              className="flex min-w-0 items-center gap-1.5 rounded-lg px-1.5 py-1 text-xs font-medium text-muted transition-colors hover:bg-hover hover:text-fg"
            >
              <UserIcon className="size-3.5" />
              <span className="truncate">Sign in</span>
            </button>
          )}
          <ThemeQuickSwitch />
        </div>
      </div>
    </aside>
  )
}

const THEME_OPTIONS: readonly { mode: ThemeMode; label: string; icon: ReactNode }[] = [
  { mode: 'system', label: 'System theme', icon: <MonitorIcon className="size-3.5" /> },
  { mode: 'light', label: 'Light theme', icon: <SunIcon className="size-3.5" /> },
  { mode: 'dark', label: 'Dark theme', icon: <MoonIcon className="size-3.5" /> },
]

function ThemeQuickSwitch() {
  const mode = useTheme((s) => s.mode)
  const setMode = useTheme((s) => s.setMode)
  return (
    <div
      role="group"
      aria-label="Appearance"
      className="inline-flex shrink-0 items-center rounded-lg border border-line bg-bg/70 p-0.5"
    >
      {THEME_OPTIONS.map((o) => {
        const active = mode === o.mode
        return (
          <button
            key={o.mode}
            type="button"
            title={o.label}
            aria-label={o.label}
            aria-pressed={active}
            onClick={() => setMode(o.mode)}
            className={`inline-flex size-6 items-center justify-center rounded-md p-0 leading-none transition-all ${
              active
                ? 'bg-surface text-accent-text shadow-theme-xs ring-1 ring-line-strong'
                : 'text-muted hover:text-fg'
            }`}
          >
            {o.icon}
          </button>
        )
      })}
    </div>
  )
}

function StatusBar() {
  const { busy, progress, notice, set } = useApp()
  // Success notices clear 5s after they can be seen (window focused); the cleanup keeps an
  // old timer off a newer notice.
  useEffect(() => {
    if (notice?.kind !== 'ok') return
    let timer: ReturnType<typeof setTimeout> | undefined
    const start = () => {
      timer ??= setTimeout(() => set({ notice: null }), 5000)
    }
    if (document.hasFocus()) start()
    window.addEventListener('focus', start)
    return () => {
      clearTimeout(timer)
      window.removeEventListener('focus', start)
    }
  }, [notice, set])
  if (!busy && !notice) return null
  return (
    <div
      className={`flex h-9 shrink-0 items-center gap-3 border-t px-4 text-xs ${
        !busy && notice?.kind === 'error'
          ? 'border-bad-border bg-bad-soft text-bad'
          : !busy && notice?.kind === 'ok'
            ? 'border-ok-border bg-ok-soft text-ok'
            : 'border-line bg-sidebar text-fg'
      }`}
    >
      {busy ? (
        <>
          <span className="size-2 shrink-0 rounded-full bg-accent" />
          <span className="font-medium text-fg">{busy}</span>
          {progress && progress.total > 0 && (
            <>
              <ProgressBar done={progress.done} total={progress.total} />
              <span className="font-mono text-[11px] text-muted tabular-nums">
                {progress.done}/{progress.total}
              </span>
              <span className="truncate text-muted">{progress.label}</span>
            </>
          )}
        </>
      ) : notice ? (
        <>
          {notice.kind === 'error' ? <AlertIcon className="size-3.5 shrink-0" /> : <CheckIcon className="size-3.5 shrink-0" />}
          <span className="min-w-0 flex-1 truncate font-medium" title={notice.text}>
            {notice.text}
          </span>
          {notice.open && (
            <Button small variant="default" onClick={() => void call('open_external', { target: notice.open!.path })}>
              {notice.open.label}
            </Button>
          )}
          <button
            type="button"
            className="grid size-6 place-items-center rounded-md opacity-75 transition-opacity hover:bg-hover hover:opacity-100"
            onClick={() => set({ notice: null })}
            aria-label="Dismiss"
          >
            <CloseIcon className="size-3.5" />
          </button>
        </>
      ) : null}
    </div>
  )
}

function ConfirmDialog() {
  const confirm = useApp((s) => s.confirm)
  const set = useApp((s) => s.set)
  if (!confirm) return null
  const close = () => set({ confirm: null })
  return (
    <Dialog
      title={confirm.title}
      onClose={close}
      footer={
        <>
          <Button onClick={close}>Cancel</Button>
          <Button
            variant={confirm.danger ? 'danger' : 'primary'}
            onClick={() => {
              close()
              confirm.onConfirm()
            }}
          >
            {confirm.action}
          </Button>
        </>
      }
    >
      <p className="leading-relaxed text-fg-secondary">{confirm.body}</p>
    </Dialog>
  )
}

/** macOS draws the traffic lights over the app (`titleBarStyle: Overlay`); other platforms keep
 * their native title bar. */
const isMac = navigator.userAgent.includes('Mac')

// Colons can't appear in Minecraft names, so these never collide with an account.
const ADD = ':add'
const REMOVE = ':remove'
const MANAGE = ':manage'

/** Picks the Minecraft account Play uses. Built-in Prism accounts are added (through Prism's
 * sign-in page only) and removed here; an installed Prism manages its own. */
function MinecraftAccount() {
  const accounts = useQuery({ queryKey: keys.accounts, queryFn: () => call('minecraft_accounts'), staleTime: 5_000 })
  const invalidate = [keys.accounts]
  const add = useAction(() => call('add_minecraft_account'), {
    label: 'Waiting for the Microsoft sign-in in Prism Launcher',
    invalidate,
    done: (a) => (a ? `Playing as ${a.name}` : ''),
  })
  const remove = useAction((name: string) => call('remove_minecraft_account', { name }), { label: 'Removing account', invalidate })
  const select = useAction((name: string) => call('set_minecraft_account', { name }), { label: 'Switching account', invalidate })
  const manage = useAction(() => call('open_prism'), { label: 'Opening Prism Launcher', done: 'Manage accounts in Prism under Accounts' })
  const data = accounts.data
  const list = data?.accounts ?? []
  const builtIn = data?.prism === 'built_in'
  const addOrManage = () => (builtIn ? add.mutate(undefined) : manage.mutate(undefined))

  const pick = (value: string) => {
    if (value === ADD || value === MANAGE) return addOrManage()
    if (value === REMOVE && data?.selected) {
      const name = data.selected
      return askConfirm({
        title: `Remove ${name}?`,
        body: 'BeaconSync signs this Minecraft account out. You can add it again later.',
        action: 'Remove',
        danger: true,
        onConfirm: () => remove.mutate(name),
      })
    }
    select.mutate(value)
  }

  return (
    <div className="flex items-center gap-2 px-3 py-2 text-xs">
      {list.length === 0 ? (
        <button
          type="button"
          className="flex h-8 w-full items-center justify-center gap-1.5 rounded-lg border border-dashed border-line-strong bg-bg/50 px-2.5 text-xs font-medium text-muted transition-colors hover:border-accent-border hover:bg-accent-soft hover:text-accent-text disabled:opacity-40"
          onClick={addOrManage}
          disabled={add.isPending}
        >
          <UserIcon className="size-3.5" />
          Add Minecraft account
        </button>
      ) : (
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <span className="px-0.5 text-[10px] font-semibold uppercase tracking-[0.06em] text-subtle">Minecraft</span>
          <Select
            aria-label="Minecraft account"
            size="sm"
            value={data?.selected ?? ''}
            onChange={pick}
            className="w-full"
            options={[
              ...list.map((a) => ({ value: a.name, label: a.offline ? `${a.name} (offline)` : a.name })),
              ...(builtIn
                ? [
                    { value: ADD, label: '+ Add account' },
                    ...(data?.selected ? [{ value: REMOVE, label: `Remove ${data.selected}` }] : []),
                  ]
                : [{ value: MANAGE, label: 'Manage accounts in Prism' }]),
            ]}
          />
        </div>
      )}
    </div>
  )
}
