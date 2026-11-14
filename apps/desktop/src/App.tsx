import { useQuery, useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { useEffect } from 'react'
import logo from './assets/logo.svg'
import { Conflicts } from './components/Conflicts'
import { Button, Dialog, ProgressBar, Select } from './components/ui'
import { UpdateButton, useUpdate } from './components/UpdateButton'
import { call } from './lib/api'
import { askConfirm, keys, useAction, useApp, type View } from './lib/store'
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
    <div className="flex h-full flex-col">
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <main className="flex min-w-0 flex-1 flex-col">
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

  const nav = (name: 'library' | 'join' | 'settings', label: string) => (
    <button
      type="button"
      onClick={() => go({ name })}
      className={`flex h-7 items-center rounded-md px-2 text-left ${view.name === name ? 'bg-[#141414] text-fg' : 'text-muted hover:text-fg'}`}
    >
      {label}
    </button>
  )

  return (
    <aside className="flex w-56 shrink-0 flex-col border-r border-line">
      {isMac && <div data-tauri-drag-region className="h-7 shrink-0" />}
      <div data-tauri-drag-region className="flex h-12 items-center gap-2 px-4">
        <img src={logo} alt="" className="size-6" />
        <span className="font-semibold tracking-tight">BeaconSync</span>
        {version && (
          <span className="min-w-0 truncate text-[11px] text-[#555]" title={version}>
            {version}
          </span>
        )}
        <UpdateButton />
      </div>
      <nav className="flex flex-col gap-0.5 px-2">
        {nav('library', 'Library')}
        {nav('join', 'Join with code')}
        {nav('settings', 'Settings')}
      </nav>
      <div className="mx-4 my-3 border-t border-line" />
      <div className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-auto px-2">
        {tracked.map((i) => {
          const active = view.name === 'pack' && view.path === i.path
          return (
            <button
              key={i.path}
              type="button"
              onClick={() => go({ name: 'pack', path: i.path, tab: 'mods' })}
              className={`flex h-7 items-center justify-between gap-2 rounded-md px-2 text-left ${active ? 'bg-[#141414] text-fg' : 'text-muted hover:text-fg'}`}
            >
              <span className="truncate">{i.name}</span>
              <span className="shrink-0 text-[11px] text-[#555]">{i.role === 'subscriber' ? 'joined' : i.minecraft}</span>
            </button>
          )
        })}
      </div>
      <MinecraftAccount />
      <div className="flex h-10 items-center border-t border-line px-4 text-xs text-muted">
        {settings.data?.signed_in ? (
          <span className="truncate">
            Signed in as <span className="text-fg">{settings.data.username}</span>
          </span>
        ) : (
          <button type="button" className="hover:text-fg" onClick={() => go({ name: 'settings' })}>
            Not signed in
          </button>
        )}
      </div>
    </aside>
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
    <div className="flex h-8 shrink-0 items-center gap-3 border-t border-line px-4 text-xs">
      {busy ? (
        <>
          <span>{busy}</span>
          {progress && progress.total > 0 && (
            <>
              <ProgressBar done={progress.done} total={progress.total} />
              <span className="text-muted">
                {progress.done}/{progress.total}
              </span>
              <span className="truncate text-muted">{progress.label}</span>
            </>
          )}
        </>
      ) : notice ? (
        <>
          <span className={`min-w-0 flex-1 truncate ${notice.kind === 'error' ? 'text-bad' : 'text-ok'}`} title={notice.text}>
            {notice.text}
          </span>
          {notice.open && (
            <button type="button" className="text-muted hover:text-fg" onClick={() => void call('open_external', { target: notice.open!.path })}>
              {notice.open.label}
            </button>
          )}
          <button type="button" className="text-muted hover:text-fg" onClick={() => set({ notice: null })}>
            Dismiss
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
      <p className="text-muted">{confirm.body}</p>
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
    <div className="flex h-10 items-center gap-2 border-t border-line px-4 text-xs text-muted">
      {list.length === 0 ? (
        <button type="button" className="hover:text-fg" onClick={addOrManage} disabled={add.isPending}>
          Add Minecraft account
        </button>
      ) : (
        <>
          <span className="shrink-0">Playing as</span>
          <Select
            value={data?.selected ?? ''}
            onChange={pick}
            className="h-7 min-w-0 flex-1 text-xs text-fg"
            options={[
              ...list.map((a) => ({ value: a.name, label: a.offline ? `${a.name} (offline)` : a.name })),
              ...(builtIn
                ? [
                    { value: ADD, label: 'Add account' },
                    ...(data?.selected ? [{ value: REMOVE, label: `Remove ${data.selected}` }] : []),
                  ]
                : [{ value: MANAGE, label: 'Manage accounts in Prism' }]),
            ]}
          />
        </>
      )}
    </div>
  )
}
