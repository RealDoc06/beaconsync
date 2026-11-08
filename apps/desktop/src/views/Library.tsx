import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { InstanceIcon } from '../components/InstanceIcon'
import { PageHeader } from '../components/PageHeader'
import { Button, Empty, ErrorLine, Input, Tag } from '../components/ui'
import { call } from '../lib/api'
import { LOADER_LABEL } from '../lib/format'
import { keys, useAction, useApp } from '../lib/store'
import type { LibraryItem } from '../lib/types'

export function LibraryView() {
  const go = useApp((s) => s.go)
  const set = useApp((s) => s.set)
  const [filter, setFilter] = useState('')
  // Cheap folder listing; refetch on open so instances made in Prism show up.
  const library = useQuery({ queryKey: keys.library, queryFn: () => call('list_instances'), refetchOnMount: 'always' })
  const items = (library.data ?? []).filter((i) => i.name.toLowerCase().includes(filter.trim().toLowerCase()))

  return (
    <>
      <PageHeader title="Library">
        <Input placeholder="Filter" value={filter} onChange={(e) => setFilter(e.target.value)} className="w-48" />
        <Button onClick={() => go({ name: 'join' })}>Join with code</Button>
        <Button onClick={() => go({ name: 'import' })}>Import</Button>
        <Button variant="primary" onClick={() => set({ newPackOpen: true })}>
          New pack
        </Button>
      </PageHeader>
      <div className="min-h-0 flex-1 overflow-auto">
        <ErrorLine error={library.error} />
        {library.data && library.data.length === 0 ? (
          <Empty action={<Button variant="primary" onClick={() => set({ newPackOpen: true })}>New pack</Button>}>
            No packs yet. Create one, import a modpack or join with a code.
          </Empty>
        ) : (
          <div role="table" className="text-[13px]">
            <div className="grid grid-cols-[minmax(0,1fr)_80px_130px_50px_140px_150px] items-center gap-3 border-b border-line px-5 py-2 text-xs text-muted">
              <span>Name</span>
              <span>Minecraft</span>
              <span>Loader</span>
              <span className="text-right">Mods</span>
              <span>State</span>
              <span />
            </div>
            {items.map((i) => (
              <Row key={i.path} item={i} />
            ))}
          </div>
        )}
      </div>
    </>
  )
}

function Row({ item }: { item: LibraryItem }) {
  const go = useApp((s) => s.go)
  const open = () => item.tracked && go({ name: 'pack', path: item.path, tab: 'mods' })
  const badges = useQuery({
    queryKey: keys.badges(item.path),
    queryFn: () => call('instance_badges', { path: item.path }),
    enabled: item.tracked,
    staleTime: 120_000,
  })
  const track = useAction(() => call('adopt_instance', { dir: item.path }), {
    label: `Tracking ${item.name}`,
    invalidate: [keys.library],
    done: `${item.name} is now tracked`,
  })
  const play = useAction(() => call('launch', { path: item.path }), { label: 'Starting Minecraft', done: 'Minecraft is starting' })

  return (
    <div
      role="row"
      onDoubleClick={open}
      onClick={open}
      className={`grid h-11 grid-cols-[minmax(0,1fr)_80px_130px_50px_140px_150px] items-center gap-3 border-b border-line px-5 ${item.tracked ? 'cursor-default hover:bg-hover' : ''}`}
    >
      <div className="flex min-w-0 items-center gap-2">
        <InstanceIcon path={item.path} size={24} />
        <span className={`truncate ${item.tracked ? '' : 'text-muted'}`}>{item.name}</span>
        {item.role === 'subscriber' && <Tag>joined</Tag>}
        {item.upstream && <Tag>from {item.upstream}</Tag>}
      </div>
      <span className="text-muted">{item.minecraft}</span>
      <span className="truncate text-muted">
        {LOADER_LABEL[item.loader.kind]} {item.loader.version}
      </span>
      <span className="text-right text-muted tabular-nums">{item.mod_count}</span>
      <div className="flex gap-3">
        {!item.tracked && <Tag>Not tracked</Tag>}
        {badges.data && badges.data.unsaved > 0 && item.role !== 'subscriber' && <Tag tone="warn">{badges.data.unsaved} unsaved</Tag>}
        {badges.data?.update && <Tag tone="accent">Update available</Tag>}
      </div>
      <div className="flex justify-end gap-2" onClick={(e) => e.stopPropagation()}>
        {!item.tracked && (
          <Button small onClick={() => track.mutate(undefined)} disabled={track.isPending} title="Track with BeaconSync">
            Track
          </Button>
        )}
        <Button small onClick={() => play.mutate(undefined)} disabled={play.isPending}>
          Play
        </Button>
      </div>
    </div>
  )
}

