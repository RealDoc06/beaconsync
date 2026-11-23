import { useQuery, useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { useEffect, useState, type ReactNode } from 'react'
import { CommandPalette } from './components/CommandPalette'
import { Conflicts } from './components/Conflicts'
import {
  AlertIcon,
  CheckIcon,
  CloseIcon,
  DownloadIcon,
  LibraryIcon,
  LinkCodeIcon,
  MonitorIcon,
  MoonIcon,
  PlayIcon,
  PlusIcon,
  SearchIcon,
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
import type { GameFailure, LibraryItem, Progress, UpdateView } from './lib/types'
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
  const [paletteOpen, setPaletteOpen] = useState(false)

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

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.repeat || e.altKey || e.shiftKey) return
      const mod = isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey
      if (!mod || e.code !== 'KeyK') return
      // Don't open over an existing modal dialog (e.g. Rename, Publish, Delete confirm).
      if (!paletteOpen && document.querySelector('[role="dialog"]')) return
      e.preventDefault()
      setPaletteOpen((v) => !v)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [paletteOpen])

  return (
    <div className="flex h-full flex-col bg-shell text-fg">
      <TopBar onOpenPalette={() => setPaletteOpen(true)} />
      <div className="flex min-h-0 flex-1 gap-2 px-2 pb-2">
        <Rail />
        <main className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-2xl border border-line bg-bg shadow-theme-canvas">
          <Page view={view} />
          <StatusBar />
        </main>
      </div>
      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
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

/** Unified macOS-aligned top chrome: brand + version left, quick command bar center, accounts & theme right. */
function TopBar({ onOpenPalette }: { onOpenPalette: () => void }) {
  const view = useApp((s) => s.view)
  const go = useApp((s) => s.go)
  const library = useQuery({ queryKey: keys.library, queryFn: () => call('list_instances') })
  const settings = useQuery({ queryKey: keys.settings, queryFn: () => call('get_settings') })
  const version = useUpdate()?.current

  const activePack = view.name === 'pack' ? library.data?.find((i) => i.path === view.path) : undefined
  const locationLabel =
    view.name === 'pack'
      ? activePack?.name ?? 'Pack'
      : view.name === 'library'
        ? 'Library'
        : view.name === 'import'
          ? 'Import'
          : view.name === 'join'
            ? 'Join with code'
            : 'Settings'

  return (
    <header
      data-tauri-drag-region
      className={`flex shrink-0 items-center gap-3 pr-3 ${isMac ? 'h-9.5 pl-[76px]' : 'h-10 pl-3.5'}`}
    >
      <div data-tauri-drag-region className="flex shrink-0 items-center gap-1.5">
        <button
          type="button"
          onClick={() => go({ name: 'library' })}
          className="rounded text-[13px] font-semibold leading-none tracking-tight text-fg transition-colors hover:text-accent-text"
        >
          BeaconSync
        </button>
        {version && (
          <span className="font-mono text-[10.5px] leading-none text-subtle" title={version}>
            v{version.replace(/^v/, '')}
          </span>
        )}
        <UpdateButton />
      </div>

      <div data-tauri-drag-region className="flex min-w-0 flex-1 justify-center">
        <button
          type="button"
          onClick={onOpenPalette}
          className="flex h-7.5 w-full max-w-[300px] items-center gap-2 rounded-lg border border-line bg-bg/70 px-2.5 text-xs text-muted shadow-theme-xs transition-colors hover:border-line-strong hover:bg-surface hover:text-fg"
          title="Open quick command palette"
        >
          <SearchIcon className="size-3.5 shrink-0 text-subtle" />
          <span className="min-w-0 flex-1 truncate text-left">
            <span className="font-medium text-fg-secondary">{locationLabel}</span>
            <span className="text-subtle"> · Search or jump…</span>
          </span>
          <kbd className="shrink-0 rounded border border-line-strong bg-surface-2 px-1.5 py-0.5 font-mono text-[10px] leading-none text-muted">
            {isMac ? '⌘K' : 'Ctrl K'}
          </kbd>
        </button>
      </div>

      <div className="flex shrink-0 items-center gap-2">
        <ThemeQuickSwitch />
        <MinecraftAccount />
        {settings.data?.signed_in ? (
          <button
            type="button"
            onClick={() => go({ name: 'settings' })}
            className="inline-flex h-7.5 items-center gap-1.5 rounded-lg border border-line bg-bg/70 px-2 text-xs font-medium text-fg transition-colors hover:border-line-strong hover:bg-surface"
            title={`Signed in to hub as ${settings.data.username}`}
          >
            <span className="grid size-4.5 place-items-center rounded-full bg-accent-soft text-[10px] font-semibold uppercase text-accent-text ring-1 ring-accent-border">
              {(settings.data.username ?? '?').slice(0, 1)}
            </span>
            <span className="max-w-22 truncate">{settings.data.username}</span>
          </button>
        ) : (
          <button
            type="button"
            onClick={() => go({ name: 'settings' })}
            className="inline-flex h-7.5 items-center gap-1.5 rounded-lg border border-line bg-bg/70 px-2.5 text-xs font-medium text-muted transition-colors hover:border-line-strong hover:bg-surface hover:text-fg"
          >
            <UserIcon className="size-3.5" />
            Sign in
          </button>
        )}
      </div>
    </header>
  )
}

/** Slim floating rail on the shell background. */
function Rail() {
  const view = useApp((s) => s.view)
  const go = useApp((s) => s.go)
  const set = useApp((s) => s.set)
  const library = useQuery({ queryKey: keys.library, queryFn: () => call('list_instances') })
  const tracked = library.data?.filter((i) => i.tracked) ?? []

  const nav = (name: 'library' | 'import' | 'join' | 'settings', label: string, icon: ReactNode) => {
    const active = view.name === name
    return (
      <button
        type="button"
        onClick={() => go({ name })}
        className={`group flex h-8.5 items-center gap-2.5 rounded-xl px-2.5 text-left text-[12.5px] font-medium transition-all ${
          active
            ? 'bg-surface text-fg shadow-theme-xs ring-1 ring-line-strong'
            : 'text-muted hover:bg-hover hover:text-fg'
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
    <aside className="flex w-48 lg:w-52 shrink-0 flex-col py-1">
      <nav className="flex flex-col gap-0.5 px-1">
        {nav('library', 'Library', <LibraryIcon className="size-4" />)}
        {nav('import', 'Import', <DownloadIcon className="size-4" />)}
        {nav('join', 'Join with code', <LinkCodeIcon className="size-4" />)}
        {nav('settings', 'Settings', <SettingsIcon className="size-4" />)}
      </nav>

      <div className="mx-3 my-3 border-t border-line" />

      <div className="flex items-center justify-between px-3 pb-1.5">
        <span className="text-[10.5px] font-semibold uppercase tracking-[0.07em] text-subtle">Your packs</span>
        {tracked.length > 0 && (
          <span className="rounded-full bg-surface-2 px-1.5 py-px font-mono text-[10px] font-medium text-muted tabular-nums">
            {tracked.length}
          </span>
        )}
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-1 overflow-auto px-1 pb-2">
        {tracked.length === 0 ? (
          <div className="px-2 py-2 text-xs text-subtle">No tracked packs yet</div>
        ) : (
          tracked.map((i) => <RailPack key={i.path} item={i} active={view.name === 'pack' && view.path === i.path} />)
        )}
      </div>

      <div className="px-1 pt-2">
        <button
          type="button"
          onClick={() => set({ newPackOpen: true })}
          className="flex h-9 w-full items-center justify-center gap-1.5 rounded-xl border border-accent-border bg-accent-soft text-xs font-semibold text-accent-text transition-colors hover:bg-accent hover:text-accent-fg"
        >
          <PlusIcon className="size-3.5" />
          New pack
        </button>
      </div>
    </aside>
  )
}

function RailPack({ item, active }: { item: LibraryItem; active: boolean }) {
  const go = useApp((s) => s.go)
  const play = useAction(() => call('launch', { path: item.path }), {
    label: 'Starting Minecraft',
    done: 'Minecraft is starting',
  })
  return (
    <div
      className={`group relative flex items-center gap-1 rounded-xl pr-1 transition-all ${
        active
          ? 'bg-surface text-fg shadow-theme-xs ring-1 ring-accent-border'
          : 'text-fg-secondary hover:bg-hover hover:text-fg'
      }`}
    >
      <button
        type="button"
        onClick={() => go({ name: 'pack', path: item.path, tab: 'mods' })}
        className="flex h-9 min-w-0 flex-1 items-center gap-2 rounded-xl px-2 text-left"
      >
        <InstanceIcon path={item.path} size={22} framed={false} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-[12px] font-medium leading-tight">{item.name}</div>
          <div className="mt-0.5 truncate font-mono text-[10px] leading-none text-subtle">
            {item.role === 'subscriber' ? 'joined' : item.minecraft}
          </div>
        </div>
      </button>
      <button
        type="button"
        aria-label={`Play ${item.name}`}
        title={`Play ${item.name}`}
        disabled={play.isPending}
        onClick={() => play.mutate(undefined)}
        className="inline-flex size-6.5 shrink-0 items-center justify-center rounded-lg p-0 text-muted opacity-0 transition-all hover:bg-accent hover:text-accent-fg group-hover:opacity-100 focus-visible:opacity-100"
      >
        <PlayIcon className="size-2.5" />
      </button>
    </div>
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
      className={`flex h-9 shrink-0 items-center gap-3 border-t px-5 text-xs ${
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
            className="inline-flex size-6 items-center justify-center rounded-md p-0 opacity-75 transition-opacity hover:bg-hover hover:opacity-100"
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

/** Picks the Minecraft account Play uses. Compact pill in the top bar. */
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

  if (list.length === 0) {
    return (
      <button
        type="button"
        aria-label="Add Minecraft account"
        title="Add Minecraft account"
        className="inline-flex h-7.5 items-center gap-1.5 rounded-lg border border-dashed border-line-strong bg-bg/60 px-2.5 text-xs font-medium text-muted transition-colors hover:border-accent-border hover:bg-accent-soft hover:text-accent-text disabled:opacity-40"
        onClick={addOrManage}
        disabled={add.isPending}
      >
        <UserIcon className="size-3.5" />
        <span className="max-[980px]:hidden">Add MC account</span>
      </button>
    )
  }

  return (
    <Select
      aria-label="Minecraft account"
      size="sm"
      value={data?.selected ?? ''}
      onChange={pick}
      className="max-w-36"
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
  )
}
