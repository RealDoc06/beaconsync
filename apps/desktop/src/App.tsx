import { useQuery } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { useEffect } from 'react'
import logo from './assets/logo.svg'
import { Conflicts } from './components/Conflicts'
import { Button, Dialog, ProgressBar } from './components/ui'
import { call } from './lib/api'
import { keys, useApp, type View } from './lib/store'
import type { Progress } from './lib/types'
import { ImportView } from './views/Import'
import { JoinView } from './views/Join'
import { LibraryView } from './views/Library'
import { NewPack } from './views/NewPack'
import { PackView } from './views/Pack'
import { SettingsView } from './views/Settings'

export function App() {
  const view = useApp((s) => s.view)
  const set = useApp((s) => s.set)

  useEffect(() => {
    const off = listen<Progress>('progress', (e) => set({ progress: e.payload }))
    return () => void off.then((f) => f())
  }, [set])

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
      <div className="flex h-12 items-center gap-2 px-4">
        <img src={logo} alt="" className="size-6" />
        <span className="font-semibold tracking-tight">BeaconSync</span>
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
