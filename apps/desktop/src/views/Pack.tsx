import { useQuery } from '@tanstack/react-query'
import { useState, type ReactNode } from 'react'
import {
  CubeIcon,
  DiffIcon,
  FolderIcon,
  GitBranchIcon,
  HistoryIcon,
  PencilIcon,
  PlayIcon,
  ServerIcon,
  ShareIcon,
  SparklesIcon,
  TrashIcon,
} from '../components/Icons'
import { InstanceIcon } from '../components/InstanceIcon'
import { PageHeader } from '../components/PageHeader'
import { Button, Dialog, ErrorLine, Input, Select, Tabs, Tag } from '../components/ui'
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
  if (!p) {
    return info.error ? (
      <div className="p-6">
        <ErrorLine error={info.error} />
      </div>
    ) : null
  }

  const subscriber = p.config.role === 'subscriber'
  const changes = status.data
    ? status.data.diff.files.length + (status.data.diff.minecraft ? 1 : 0) + (status.data.diff.loader ? 1 : 0)
    : 0
  const tabs: { value: PackTab; label: ReactNode; icon: ReactNode }[] = [
    { value: 'mods', label: 'Mods', icon: <CubeIcon className="size-3.5" /> },
    ...(subscriber
      ? []
      : [
          {
            value: 'changes' as const,
            icon: <DiffIcon className="size-3.5" />,
            label: (
              <span className="flex items-center gap-1.5">
                Changes
                {changes > 0 && (
                  <span className="rounded-full bg-warn-soft px-1.5 py-px font-mono text-[10.5px] font-semibold text-warn ring-1 ring-warn-border tabular-nums">
                    {changes}
                  </span>
                )}
              </span>
            ),
          },
          { value: 'history' as const, label: 'History', icon: <HistoryIcon className="size-3.5" /> },
          ...(settings.data?.signed_in
            ? [{ value: 'servers' as const, label: 'Servers', icon: <ServerIcon className="size-3.5" /> }]
            : []),
        ]),
    { value: 'share', label: 'Share', icon: <ShareIcon className="size-3.5" /> },
  ]
  const active = tabs.some((t) => t.value === tab) ? tab : 'mods'

  return (
    <>
      <Header p={p} />
      {subscriber && <SubscribedBanner path={path} />}
      <Tabs value={active} onChange={(t) => go({ name: 'pack', path, tab: t })} tabs={tabs} />
      <div className="flex min-h-0 flex-1 flex-col">
        {active === 'mods' && <ModsTab p={p} />}
        {active === 'changes' && <ChangesTab path={path} />}
        {active === 'history' && <HistoryTab path={path} published={!!p.config.remote && !!settings.data?.signed_in} />}
        {active === 'servers' && <ServersTab p={p} />}
        {active === 'share' && <ShareTab p={p} />}
      </div>
    </>
  )
}

function Header({ p }: { p: PackInfo }) {
  const go = useApp((s) => s.go)
  const [renaming, setRenaming] = useState(false)
  const play = useAction(() => call('launch', { path: p.path }), { label: 'Starting Minecraft', done: 'Minecraft is starting' })
  const reveal = useAction(() => call('open_external', { target: p.path }), { label: 'Opening folder' })
  const remove = useAction(() => call('delete_pack', { path: p.path }), {
    label: 'Deleting pack',
    invalidate: [keys.library],
    done: `${p.name} deleted`,
    onSuccess: () => go({ name: 'library' }),
  })
  return (
    <PageHeader
      leading={<InstanceIcon path={p.path} size={42} />}
      title={
        <>
          <span className="truncate">{p.name}</span>
          {p.config.role === 'subscriber' && (
            <Tag tone="info" dot>
              Joined
            </Tag>
          )}
          {p.branch && p.branch !== 'main' && (
            <Tag tone="accent" shrink className="max-w-32">
              <GitBranchIcon className="size-3 shrink-0" />
              <span className="truncate" title={p.branch}>
                {p.branch}
              </span>
            </Tag>
          )}
        </>
      }
      subtitle={
        <span>
          Minecraft <span className="font-mono font-medium text-fg-secondary">{p.minecraft}</span> · {loaderText(p.loader)}
        </span>
      }
    >
      <div className="flex items-center gap-1 rounded-xl border border-line bg-surface/80 p-1">
        <Button size="icon" variant="ghost" onClick={() => reveal.mutate(undefined)} title="Open pack folder" aria-label="Open pack folder">
          <FolderIcon className="size-3.5" />
        </Button>
        <Button size="icon" variant="ghost" onClick={() => setRenaming(true)} title="Rename pack" aria-label="Rename pack">
          <PencilIcon className="size-3.5" />
        </Button>
        <Button
          size="icon"
          variant="ghost-danger"
          title="Delete pack"
          aria-label="Delete pack"
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
          <TrashIcon className="size-3.5" />
        </Button>
      </div>

      <Memory path={p.path} />

      <Button variant="primary" onClick={() => play.mutate(undefined)} disabled={play.isPending}>
        <PlayIcon className="size-3.5" />
        Play
      </Button>
      {renaming && <RenameDialog p={p} onClose={() => setRenaming(false)} />}
    </PageHeader>
  )
}

const MEMORY_GB = [2, 3, 4, 6, 8, 12, 16]

/** Memory the game gets, saved in the instance for Prism. Empty means Prism's default. */
function Memory({ path }: { path: string }) {
  const memory = useQuery({ queryKey: keys.memory(path), queryFn: () => call('instance_memory', { path }) })
  const save = useAction((mb: number | null) => call('set_instance_memory', { path, mb }), {
    label: 'Saving memory',
    invalidate: [keys.memory(path)],
  })
  if (memory.isPending) return null
  return (
    <Select
      aria-label="Minecraft memory"
      size="sm"
      value={memory.data ? String(memory.data) : ''}
      onChange={(v) => save.mutate(v ? Number(v) : null)}
      options={[
        { value: '', label: 'RAM: default' },
        ...MEMORY_GB.map((gb) => ({ value: String(gb * 1024), label: `RAM: ${gb} GB` })),
        // Keep a value set elsewhere (in Prism) selectable.
        ...(memory.data && !MEMORY_GB.includes(memory.data / 1024)
          ? [{ value: String(memory.data), label: `RAM: ${memory.data} MB` }]
          : []),
      ]}
    />
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
      <Input
        className="w-full"
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => e.key === 'Enter' && name.trim() && rename.mutate(undefined)}
      />
    </Dialog>
  )
}

/** Subscribed packs follow the owner's releases. */
function SubscribedBanner({ path }: { path: string }) {
  const release = useQuery({
    queryKey: [...keys.pack(path), 'release'],
    queryFn: () => call('check_release', { path }),
    staleTime: 30_000,
    refetchInterval: 60_000,
  })
  const update = useAction(() => call('update_subscribed', { path }), {
    label: 'Updating',
    invalidate: packKeys(path),
    done: (r) => `Updated to ${r.name}`,
  })
  const r = release.data
  return (
    <div
      className={`flex h-10 shrink-0 items-center gap-3 border-b px-6 text-xs ${
        r?.available ? 'border-accent-border bg-accent-soft text-accent-text' : 'border-line bg-sidebar/50 text-muted'
      }`}
    >
      {r?.available ? (
        <>
          <SparklesIcon className="size-3.5 shrink-0" />
          <span className="font-medium">Update available: {r.name}</span>
          <Button small variant="primary" className="ml-auto" onClick={() => update.mutate(undefined)} disabled={update.isPending}>
            Update now
          </Button>
        </>
      ) : (
        <span>
          {r
            ? `Up to date${r.name ? ` with ${r.name}` : ''}`
            : release.error
              ? 'Could not check for updates'
              : 'Checking for updates…'}
        </span>
      )}
    </div>
  )
}
