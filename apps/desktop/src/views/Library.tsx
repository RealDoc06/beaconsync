import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import {
  CubeIcon,
  DownloadIcon,
  GridViewIcon,
  LinkCodeIcon,
  ListViewIcon,
  PlayIcon,
  PlusIcon,
  SearchIcon,
} from '../components/Icons'
import { InstanceIcon } from '../components/InstanceIcon'
import { PageHeader } from '../components/PageHeader'
import { Button, Empty, ErrorLine, Input, Segmented, Tag } from '../components/ui'
import { call } from '../lib/api'
import { LOADER_LABEL } from '../lib/format'
import { keys, useAction, useApp } from '../lib/store'
import type { LibraryItem } from '../lib/types'

type Layout = 'grid' | 'list'
type Scope = 'all' | 'tracked' | 'untracked'

const LAYOUT_KEY = 'beaconsync-library-layout'

function readLayout(): Layout {
  try {
    return window.localStorage.getItem(LAYOUT_KEY) === 'list' ? 'list' : 'grid'
  } catch {
    return 'grid'
  }
}

const LIST_COLS =
  'grid-cols-[minmax(140px,1fr)_72px_124px_48px_132px_auto] @max-[760px]:grid-cols-[minmax(140px,1fr)_64px_120px_auto]'

export function LibraryView() {
  const go = useApp((s) => s.go)
  const set = useApp((s) => s.set)
  const [filter, setFilter] = useState('')
  const [scope, setScope] = useState<Scope>('all')
  const [layout, setLayout] = useState<Layout>(readLayout)

  // Cheap folder listing; refetch on open so instances made in Prism show up.
  const library = useQuery({ queryKey: keys.library, queryFn: () => call('list_instances'), refetchOnMount: 'always' })
  const all = library.data ?? []
  const trackedCount = all.filter((i) => i.tracked).length
  const untrackedCount = all.length - trackedCount
  const showScope = untrackedCount > 0 && trackedCount > 0
  const effectiveScope: Scope = showScope ? scope : 'all'

  const q = filter.trim().toLowerCase()
  const items = all.filter((i) => {
    if (effectiveScope === 'tracked' && !i.tracked) return false
    if (effectiveScope === 'untracked' && i.tracked) return false
    return !q || i.name.toLowerCase().includes(q) || i.minecraft.toLowerCase().includes(q)
  })

  const pickLayout = (next: Layout) => {
    setLayout(next)
    try {
      window.localStorage.setItem(LAYOUT_KEY, next)
    } catch {
      // Ignore storage errors.
    }
  }

  return (
    <>
      <PageHeader
        title="Library"
        subtitle={
          all.length > 0 ? `${all.length} instance${all.length === 1 ? '' : 's'} · ${trackedCount} tracked` : undefined
        }
      >
        <Button onClick={() => go({ name: 'join' })}>
          <LinkCodeIcon className="size-3.5" />
          Join with code
        </Button>
        <Button onClick={() => go({ name: 'import' })}>
          <DownloadIcon className="size-3.5" />
          Import
        </Button>
        <Button variant="primary" onClick={() => set({ newPackOpen: true })}>
          <PlusIcon className="size-3.5" />
          New pack
        </Button>
      </PageHeader>

      {all.length > 0 && (
        <div className="flex flex-wrap items-center gap-2.5 border-b border-line px-6 py-3">
          <Input
            icon={<SearchIcon className="size-3.5" />}
            placeholder="Filter packs or versions"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            className="w-56"
          />
          {showScope && (
            <Segmented
              value={effectiveScope}
              onChange={setScope}
              options={[
                { value: 'all', label: `All (${all.length})` },
                { value: 'tracked', label: `Tracked (${trackedCount})` },
                { value: 'untracked', label: `Prism (${untrackedCount})` },
              ]}
            />
          )}
          <span className="flex-1" />
          <div className="inline-flex h-8.5 items-center rounded-lg border border-line bg-sidebar p-0.5">
            <button
              type="button"
              aria-label="Grid view"
              aria-pressed={layout === 'grid'}
              onClick={() => pickLayout('grid')}
              className={`grid h-full w-7.5 place-items-center rounded-md transition-all ${
                layout === 'grid'
                  ? 'bg-surface text-fg shadow-theme-xs ring-1 ring-line-strong'
                  : 'text-muted hover:text-fg'
              }`}
            >
              <GridViewIcon className="size-3.5" />
            </button>
            <button
              type="button"
              aria-label="List view"
              aria-pressed={layout === 'list'}
              onClick={() => pickLayout('list')}
              className={`grid h-full w-7.5 place-items-center rounded-md transition-all ${
                layout === 'list'
                  ? 'bg-surface text-fg shadow-theme-xs ring-1 ring-line-strong'
                  : 'text-muted hover:text-fg'
              }`}
            >
              <ListViewIcon className="size-3.5" />
            </button>
          </div>
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-auto">
        {library.error && (
          <div className="p-6 pb-0">
            <ErrorLine error={library.error} />
          </div>
        )}
        {library.data && all.length === 0 ? (
          <Empty
            icon={<CubeIcon className="size-5" />}
            title="Your library is empty"
            action={
              <div className="flex items-center gap-2">
                <Button variant="primary" onClick={() => set({ newPackOpen: true })}>
                  <PlusIcon className="size-3.5" />
                  New pack
                </Button>
                <Button onClick={() => go({ name: 'import' })}>Import</Button>
                <Button onClick={() => go({ name: 'join' })}>Join with code</Button>
              </div>
            }
          >
            Create a pack from scratch, import a Modrinth or CurseForge modpack, or join a friend's pack with a share code.
          </Empty>
        ) : items.length === 0 && library.data ? (
          <Empty icon={<SearchIcon className="size-5" />} title="No matching packs">
            Try another search term or switch the filter above.
          </Empty>
        ) : layout === 'grid' ? (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(270px,1fr))] gap-4 p-6">
            {items.map((i) => (
              <PackCard key={i.path} item={i} />
            ))}
          </div>
        ) : (
          <div className="@container p-6">
            <div className="overflow-x-auto rounded-2xl border border-line bg-surface shadow-theme-xs text-[13px]">
              <div
                className={`grid ${LIST_COLS} items-center gap-3 border-b border-line bg-sidebar/60 px-4 py-2.5 text-[11px] font-semibold uppercase tracking-[0.05em] text-muted`}
              >
                <span>Name</span>
                <span>MC</span>
                <span className="@max-[760px]:hidden">Loader</span>
                <span className="text-right @max-[760px]:hidden">Mods</span>
                <span>Status</span>
                <span className="text-right">Actions</span>
              </div>
              {items.map((i) => (
                <Row key={i.path} item={i} />
              ))}
            </div>
          </div>
        )}
      </div>
    </>
  )
}

function usePackRowActions(item: LibraryItem) {
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
  return { open, badges, track, play }
}

function PackCard({ item }: { item: LibraryItem }) {
  const { open, badges, track, play } = usePackRowActions(item)

  return (
    <div
      className={`group relative flex flex-col justify-between gap-4 rounded-2xl border bg-surface p-4 shadow-theme-xs transition-all duration-150 ${
        item.tracked
          ? 'border-line hover:-translate-y-0.5 hover:border-accent-border hover:shadow-theme-sm'
          : 'border-line/80 bg-surface/65'
      }`}
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-start gap-3">
          <InstanceIcon path={item.path} size={42} />
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              {item.tracked ? (
                <button
                  type="button"
                  onClick={open}
                  className="max-w-full truncate rounded text-left text-[14px] font-semibold tracking-tight text-fg after:absolute after:inset-0 after:rounded-2xl group-hover:text-accent-text"
                >
                  {item.name}
                </button>
              ) : (
                <h2 className="truncate text-[14px] font-semibold tracking-tight text-fg-secondary">{item.name}</h2>
              )}
            </div>
            <div className="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-muted">
              <span className="font-mono text-[11.5px] text-fg-secondary">{item.minecraft}</span>
              <span className="text-subtle">·</span>
              <span className="truncate">
                {LOADER_LABEL[item.loader.kind]}
                {item.loader.version ? ` ${item.loader.version}` : ''}
              </span>
            </div>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-1.5">
          {!item.tracked && <Tag tone="muted">Untracked Prism</Tag>}
          {item.role === 'subscriber' && (
            <Tag tone="info" dot>
              Joined
            </Tag>
          )}
          {item.upstream && <Tag tone="muted">from {item.upstream}</Tag>}
          {badges.data && badges.data.unsaved > 0 && item.role !== 'subscriber' && (
            <Tag tone="warn" dot>
              {badges.data.unsaved} unsaved
            </Tag>
          )}
          {badges.data?.update && (
            <Tag tone="info" dot>
              Update available
            </Tag>
          )}
        </div>
      </div>

      <div className="relative z-10 flex items-center justify-between gap-2 border-t border-line pt-3">
        <span className="text-xs text-muted tabular-nums">
          <span className="font-medium text-fg-secondary">{item.mod_count}</span> mod{item.mod_count === 1 ? '' : 's'}
        </span>
        <div className="flex items-center gap-1.5">
          {!item.tracked && (
            <Button small onClick={() => track.mutate(undefined)} disabled={track.isPending} title="Track with BeaconSync">
              Track
            </Button>
          )}
          <Button
            small
            variant={item.tracked ? 'primary' : 'default'}
            onClick={() => play.mutate(undefined)}
            disabled={play.isPending}
          >
            <PlayIcon className="size-3" />
            Play
          </Button>
        </div>
      </div>
    </div>
  )
}

function Row({ item }: { item: LibraryItem }) {
  const { open, badges, track, play } = usePackRowActions(item)

  return (
    <div
      className={`group relative grid min-h-13 ${LIST_COLS} items-center gap-3 border-b border-line px-4 py-2 last:border-b-0 transition-colors ${
        item.tracked ? 'hover:bg-hover' : 'opacity-90'
      }`}
    >
      <div className="flex min-w-0 items-center gap-2.5">
        <InstanceIcon path={item.path} size={28} />
        {item.tracked ? (
          <button
            type="button"
            onClick={open}
            className="min-w-0 truncate rounded text-left font-medium text-fg after:absolute after:inset-0 group-hover:text-accent-text"
          >
            {item.name}
          </button>
        ) : (
          <span className="min-w-0 truncate font-medium text-muted">{item.name}</span>
        )}
        {item.role === 'subscriber' && <Tag tone="info">Joined</Tag>}
        {item.upstream && <Tag className="@max-[860px]:hidden">from {item.upstream}</Tag>}
      </div>
      <span className="truncate font-mono text-xs text-fg-secondary">{item.minecraft}</span>
      <span className="truncate text-xs text-muted @max-[760px]:hidden">
        {LOADER_LABEL[item.loader.kind]} {item.loader.version}
      </span>
      <span className="text-right font-mono text-xs text-muted tabular-nums @max-[760px]:hidden">{item.mod_count}</span>
      <div className="flex min-w-0 flex-wrap gap-1.5">
        {!item.tracked && <Tag>Not tracked</Tag>}
        {badges.data && badges.data.unsaved > 0 && item.role !== 'subscriber' && (
          <Tag tone="warn" dot>
            {badges.data.unsaved} unsaved
          </Tag>
        )}
        {badges.data?.update && (
          <Tag tone="info" dot>
            Update
          </Tag>
        )}
      </div>
      <div className="relative z-10 flex justify-end gap-1.5">
        {!item.tracked && (
          <Button small onClick={() => track.mutate(undefined)} disabled={track.isPending} title="Track with BeaconSync">
            Track
          </Button>
        )}
        <Button small variant={item.tracked ? 'primary' : 'default'} onClick={() => play.mutate(undefined)} disabled={play.isPending}>
          <PlayIcon className="size-3" />
          Play
        </Button>
      </div>
    </div>
  )
}
