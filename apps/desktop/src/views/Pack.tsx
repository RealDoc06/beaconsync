import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { PageHeader } from '../components/PageHeader'
import { Button, Dialog, ErrorLine, Input, Tabs } from '../components/ui'
import { call } from '../lib/api'
import { loaderText } from '../lib/format'
import { askConfirm, keys, packKeys, useAction, useApp, type PackTab } from '../lib/store'
import type { PackInfo } from '../lib/types'
import { ChangesTab } from './pack/Changes'
import { HistoryTab } from './pack/History'
import { ModsTab } from './pack/Mods'
import { ServersTab } from './pack/Servers'
import { ShareTab } from './pack/Share'

/** Query for one pack; keys live under `['pack', path]` so edits refresh every tab. */
export function usePackInfo(path: string) {
  return useQuery({ queryKey: [...keys.pack(path), 'info'], queryFn: () => call('pack_info', { path }) })
}

export function useStatus(path: string) {
  return useQuery({ queryKey: [...keys.pack(path), 'status'], queryFn: () => call('status', { path }), staleTime: 5_000 })
}

export function PackView({ path, tab }: { path: string; tab: PackTab }) {
  const go = useApp((s) => s.go)
  const info = usePackInfo(path)
  const status = useStatus(path)
  const settings = useQuery({ queryKey: keys.settings, queryFn: () => call('get_settings') })
  const p = info.data
  if (!p) return <ErrorLine error={info.error} />

  const subscriber = p.config.role === 'subscriber'
  const changes = status.data ? status.data.diff.files.length + (status.data.diff.minecraft ? 1 : 0) + (status.data.diff.loader ? 1 : 0) : 0
  const tabs: { value: PackTab; label: React.ReactNode }[] = [
    { value: 'mods', label: 'Mods' },
    ...(subscriber
      ? []
      : [
          { value: 'changes' as const, label: <>Changes{changes > 0 && <span className="text-warn tabular-nums">{changes}</span>}</> },
          { value: 'history' as const, label: 'History' },
          ...(settings.data?.signed_in ? [{ value: 'servers' as const, label: 'Servers' }] : []),
        ]),
    { value: 'share', label: 'Share' },
  ]
  const active = tabs.some((t) => t.value === tab) ? tab : 'mods'

  return (
    <>
      <Header p={p} />
      {subscriber && <SubscribedBanner path={path} />}
      <div className="pt-1">
        <Tabs value={active} onChange={(t) => go({ name: 'pack', path, tab: t })} tabs={tabs} />
      </div>
      <div className="flex min-h-0 flex-1 flex-col">
        {active === 'mods' && <ModsTab p={p} />}
        {active === 'changes' && <ChangesTab path={path} />}
        {active === 'history' && <HistoryTab path={path} />}
        {active === 'servers' && <ServersTab p={p} />}
        {active === 'share' && <ShareTab p={p} />}
      </div>
    </>
  )
}

function Header({ p }: { p: PackInfo }) {
  const go = useApp((s) => s.go)
  const [renaming, setRenaming] = useState(false)
  const play = useAction(() => call('launch', { path: p.path }), { label: 'Starting Prism Launcher', done: 'Prism Launcher started' })
  const reveal = useAction(() => call('open_external', { target: p.path }), { label: 'Opening folder' })
  const remove = useAction(() => call('delete_pack', { path: p.path }), {
    label: 'Deleting pack',
    invalidate: [keys.library],
    done: `${p.name} deleted`,
    onSuccess: () => go({ name: 'library' }),
  })
  return (
    <PageHeader
      title={
        <span className="flex items-baseline gap-3">
          <span>{p.name}</span>
          <span className="text-xs font-normal text-muted">
            Minecraft {p.minecraft} · {loaderText(p.loader)}
            {p.branch && p.branch !== 'main' && ` · ${p.branch}`}
          </span>
        </span>
      }
    >
      <Button variant="ghost" onClick={() => setRenaming(true)}>
        Rename
      </Button>
      <Button
        variant="ghost"
        onClick={() =>
          askConfirm({
            title: `Delete ${p.name}?`,
            body: 'This removes the instance folder, its worlds and its saved versions from this computer. Published copies on the hub stay.',
            action: 'Delete',
            danger: true,
            onConfirm: () => remove.mutate(undefined),
          })
        }
      >
        Delete
      </Button>
      <Button onClick={() => reveal.mutate(undefined)}>Open folder</Button>
      <Button variant="primary" onClick={() => play.mutate(undefined)} disabled={play.isPending}>
        Play
      </Button>
      {renaming && <RenameDialog p={p} onClose={() => setRenaming(false)} />}
    </PageHeader>
  )
}

function RenameDialog({ p, onClose }: { p: PackInfo; onClose: () => void }) {
  const [name, setName] = useState(p.name)
  const rename = useAction(() => call('rename_pack', { path: p.path, name }), {
    label: 'Renaming',
    invalidate: packKeys(p.path),
    onSuccess: onClose,
  })
  return (
    <Dialog
      title="Rename pack"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!name.trim()} onClick={() => rename.mutate(undefined)}>
            Rename
          </Button>
        </>
      }
    >
      <Input className="w-full" value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && rename.mutate(undefined)} />
    </Dialog>
  )
}

/** Subscribed packs follow the owner's releases. */
function SubscribedBanner({ path }: { path: string }) {
  const release = useQuery({
    queryKey: [...keys.pack(path), 'release'],
    queryFn: () => call('check_release', { path }),
    staleTime: 120_000,
  })
  const update = useAction(() => call('update_subscribed', { path }), {
    label: 'Updating',
    invalidate: packKeys(path),
    done: (r) => `Updated to ${r.name}`,
  })
  const r = release.data
  return (
    <div className="flex h-10 items-center gap-3 border-b border-line px-5 text-xs">
      {r?.available ? (
        <>
          <span className="text-accent">Update available: {r.name}</span>
          <Button small variant="primary" onClick={() => update.mutate(undefined)} disabled={update.isPending}>
            Update
          </Button>
        </>
      ) : (
        <span className="text-muted">{r ? `Up to date${r.name ? ` with ${r.name}` : ''}` : release.error ? 'Could not check for updates' : 'Checking for updates'}</span>
      )}
    </div>
  )
}
