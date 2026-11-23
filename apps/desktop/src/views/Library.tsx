import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import {
  ChevronRightIcon,
  CubeIcon,
  DiffIcon,
  DownloadIcon,
  GridViewIcon,
  LinkCodeIcon,
  ListViewIcon,
  PlayIcon,
  PlusIcon,
  SearchIcon,
  ServerIcon,
} from '../components/Icons'
import { InstanceIcon } from '../components/InstanceIcon'
import { Button, Empty, ErrorLine, Input, Tag } from '../components/ui'
import { call } from '../lib/api'
import { LOADER_LABEL } from '../lib/format'
import { keys, useAction, useApp } from '../lib/store'
import type { LibraryItem, LoaderKind } from '../lib/types'

type Layout = 'grid' | 'list'

const LAYOUT_KEY = 'beaconsync-library-layout'

function readLayout(): Layout {
  try {
    return window.localStorage.getItem(LAYOUT_KEY) === 'list' ? 'list' : 'grid'
  } catch {
    return 'grid'
  }
}

/** Soft deterministic cover band tint per mod loader so every pack card has distinct character. */
const LOADER_BANNER: Record<LoaderKind, string> = {
  fabric: 'from-accent/18 via-accent/6 to-transparent',
  neoforge: 'from-bad/15 via-warn/8 to-transparent',
  forge: 'from-info/16 via-info/5 to-transparent',
  quilt: 'from-info/14 via-accent/8 to-transparent',
  vanilla: 'from-ok/16 via-ok/5 to-transparent',
}

const LIST_COLS =
  'grid-cols-[minmax(140px,1fr)_72px_124px_48px_132px_auto] @max-[760px]:grid-cols-[minmax(140px,1fr)_64px_120px_auto]'

export function LibraryView() {
  const go = useApp((s) => s.go)
  const set = useApp((s) => s.set)
  const [filter, setFilter] = useState('')
  const [layout, setLayout] = useState<Layout>(readLayout)

  // Cheap folder listing; refetch on open so instances made in Prism show up.
  const library = useQuery({ queryKey: keys.library, queryFn: () => call('list_instances'), refetchOnMount: 'always' })
  const all = library.data ?? []
  const q = filter.trim().toLowerCase()
  const filtered = all.filter((i) => !q || i.name.toLowerCase().includes(q) || i.minecraft.toLowerCase().includes(q))
  const tracked = filtered.filter((i) => i.tracked)
  const untracked = filtered.filter((i) => !i.tracked)

  // Show the featured Hero Stage when there is at least one tracked pack and no active search filter.
  const hero = !q && layout === 'grid' ? tracked[0] : undefined
  const restTracked = hero ? tracked.slice(1) : tracked

  const pickLayout = (next: Layout) => {
    setLayout(next)
    try {
      window.localStorage.setItem(LAYOUT_KEY, next)
    } catch {
      // Ignore storage errors.
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Studio Gallery Top Toolbar */}
      <div className="flex flex-wrap items-center gap-2.5 border-b border-line px-6 py-3.5">
        <div className="mr-auto flex items-baseline gap-2.5">
          <h1 className="text-[18px] font-semibold tracking-tight text-fg">Library</h1>
          {all.length > 0 && (
            <span className="text-xs text-muted">
              {all.filter((i) => i.tracked).length} tracked
              {all.some((i) => !i.tracked) ? ` · ${all.filter((i) => !i.tracked).length} in Prism` : ''}
            </span>
          )}
        </div>

        {all.length > 0 && (
          <>
            <Input
              icon={<SearchIcon className="size-3.5" />}
              placeholder="Filter packs…"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              className="w-48"
            />
            <div className="inline-flex h-8.5 items-center rounded-lg border border-line bg-sidebar p-0.5">
              <button
                type="button"
                aria-label="Gallery view"
                aria-pressed={layout === 'grid'}
                onClick={() => pickLayout('grid')}
                className={`inline-flex h-full w-7.5 items-center justify-center rounded-md p-0 transition-all ${
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
                className={`inline-flex h-full w-7.5 items-center justify-center rounded-md p-0 transition-all ${
                  layout === 'list'
                    ? 'bg-surface text-fg shadow-theme-xs ring-1 ring-line-strong'
                    : 'text-muted hover:text-fg'
                }`}
              >
                <ListViewIcon className="size-3.5" />
              </button>
            </div>
          </>
        )}

        <Button variant="primary" onClick={() => set({ newPackOpen: true })}>
          <PlusIcon className="size-3.5" />
          New pack
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        {library.error && (
          <div className="p-6 pb-0">
            <ErrorLine error={library.error} />
          </div>
        )}

        {library.data && all.length === 0 ? (
          <Empty
            icon={<CubeIcon className="size-5 text-accent-text" />}
            title="Start your first modpack"
            action={
              <div className="flex flex-wrap items-center justify-center gap-2">
                <Button variant="primary" onClick={() => set({ newPackOpen: true })}>
                  <PlusIcon className="size-3.5" />
                  New pack
                </Button>
                <Button onClick={() => go({ name: 'import' })}>
                  <DownloadIcon className="size-3.5" />
                  Import
                </Button>
                <Button onClick={() => go({ name: 'join' })}>
                  <LinkCodeIcon className="size-3.5" />
                  Join with code
                </Button>
              </div>
            }
          >
            Create a pack from scratch, import a Modrinth or CurseForge modpack, or join a friend's pack with a share code.
          </Empty>
        ) : filtered.length === 0 && library.data ? (
          <Empty icon={<SearchIcon className="size-5" />} title="No matching packs">
            No packs match "{filter.trim()}".
          </Empty>
        ) : layout === 'list' ? (
          <div className="@container flex flex-col gap-5 p-6">
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
              {filtered.map((i) => (
                <Row key={i.path} item={i} />
              ))}
            </div>
          </div>
        ) : (
          <div className="flex flex-col gap-6 p-6">
            {hero && <HeroPack item={hero} />}

            {restTracked.length > 0 && (
              <section className="flex flex-col gap-3">
                {hero && (
                  <h2 className="text-[11px] font-semibold uppercase tracking-[0.07em] text-subtle">
                    More tracked packs ({restTracked.length})
                  </h2>
                )}
                <div className="grid grid-cols-[repeat(auto-fill,minmax(256px,1fr))] gap-4">
                  {restTracked.map((i) => (
                    <PackCard key={i.path} item={i} />
                  ))}
                </div>
              </section>
            )}

            {untracked.length > 0 && (
              <section className="flex flex-col gap-3">
                <div className="flex items-center justify-between">
                  <h2 className="text-[11px] font-semibold uppercase tracking-[0.07em] text-subtle">
                    Detected in Prism Launcher ({untracked.length})
                  </h2>
                  <span className="text-xs text-muted">Track any instance to version, share, and deploy it</span>
                </div>
                <div className="grid grid-cols-[repeat(auto-fill,minmax(256px,1fr))] gap-3.5">
                  {untracked.map((i) => (
                    <PackCard key={i.path} item={i} />
                  ))}
                </div>
              </section>
            )}
          </div>
        )}
      </div>
    </div>
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
  return { go, open, badges, track, play }
}

/** Atmospheric featured spotlight banner for the primary tracked pack. */
function HeroPack({ item }: { item: LibraryItem }) {
  const { go, open, badges, play } = usePackRowActions(item)
  const settings = useQuery({ queryKey: keys.settings, queryFn: () => call('get_settings') })
  const subscriber = item.role === 'subscriber'

  return (
    <section className="bg-hero-glow relative overflow-hidden rounded-2xl border border-line-strong bg-surface p-5 shadow-theme-sm">
      <div className="flex flex-wrap items-center gap-5">
        <InstanceIcon path={item.path} size={60} />

        <div className="min-w-[200px] flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-[10.5px] font-semibold uppercase tracking-[0.08em] text-accent-text">Featured Pack</span>
            {subscriber && (
              <Tag tone="info" dot>
                Joined
              </Tag>
            )}
            {item.upstream && <Tag tone="muted">from {item.upstream}</Tag>}
            {badges.data && badges.data.unsaved > 0 && !subscriber && (
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

          <h2 className="mt-1 truncate text-xl font-bold tracking-tight text-fg">{item.name}</h2>

          <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted">
            <span>
              Minecraft <span className="font-mono font-medium text-fg-secondary">{item.minecraft}</span>
            </span>
            <span className="text-subtle">·</span>
            <span>
              {LOADER_LABEL[item.loader.kind]}
              {item.loader.version ? ` ${item.loader.version}` : ''}
            </span>
            <span className="text-subtle">·</span>
            <span className="tabular-nums">
              <span className="font-medium text-fg-secondary">{item.mod_count}</span> mod{item.mod_count === 1 ? '' : 's'}
            </span>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <Button variant="primary" size="lg" onClick={() => play.mutate(undefined)} disabled={play.isPending}>
            <PlayIcon className="size-3.5" />
            Play
          </Button>
          <Button size="lg" onClick={open}>
            Open studio
            <ChevronRightIcon className="size-3.5" />
          </Button>
        </div>
      </div>

      {/* Quick-jump studio shortcuts */}
      <div className="mt-4 flex flex-wrap items-center gap-1.5 border-t border-line pt-3.5">
        <Button small variant="ghost" onClick={() => go({ name: 'pack', path: item.path, tab: 'mods' })}>
          <CubeIcon className="size-3.5" />
          Mods
        </Button>
        {!subscriber && (
          <>
            <Button small variant="ghost" onClick={() => go({ name: 'pack', path: item.path, tab: 'changes' })}>
              <DiffIcon className="size-3.5" />
              Changes
            </Button>
            {settings.data?.signed_in && (
              <Button small variant="ghost" onClick={() => go({ name: 'pack', path: item.path, tab: 'servers' })}>
                <ServerIcon className="size-3.5" />
                Servers
              </Button>
            )}
          </>
        )}
      </div>
    </section>
  )
}

function PackCard({ item }: { item: LibraryItem }) {
  const { open, badges, track, play } = usePackRowActions(item)

  return (
    <div
      className={`group relative flex flex-col justify-between overflow-hidden rounded-2xl border bg-surface shadow-theme-xs transition-all duration-150 ${
        item.tracked
          ? 'border-line hover:-translate-y-0.5 hover:border-accent-border hover:shadow-theme-sm'
          : 'border-line/80 bg-surface/75'
      }`}
    >
      {/* Subtle deterministic cover ribbon */}
      <div className={`h-12 bg-gradient-to-br ${LOADER_BANNER[item.loader.kind]} border-b border-line/60 px-4 pt-2.5`}>
        <div className="flex items-center justify-end gap-1.5">
          <span className="rounded-md bg-bg/75 px-1.5 py-0.5 font-mono text-[10.5px] font-medium text-fg-secondary ring-1 ring-line">
            {item.minecraft}
          </span>
          <span className="rounded-md bg-bg/75 px-1.5 py-0.5 text-[10.5px] font-medium text-muted ring-1 ring-line">
            {LOADER_LABEL[item.loader.kind]}
          </span>
        </div>
      </div>

      <div className="flex flex-1 flex-col justify-between gap-3.5 px-4 pb-4">
        <div className="-mt-6 flex flex-col gap-2.5">
          <InstanceIcon path={item.path} size={44} />

          <div className="min-w-0">
            {item.tracked ? (
              <button
                type="button"
                onClick={open}
                className="max-w-full truncate rounded text-left text-[14px] font-semibold tracking-tight text-fg after:absolute after:inset-0 after:rounded-2xl group-hover:text-accent-text"
              >
                {item.name}
              </button>
            ) : (
              <h3 className="truncate text-[14px] font-semibold tracking-tight text-fg-secondary">{item.name}</h3>
            )}
            <div className="mt-0.5 truncate text-xs text-muted">
              {item.loader.version ? `${LOADER_LABEL[item.loader.kind]} ${item.loader.version}` : LOADER_LABEL[item.loader.kind]}
            </div>
          </div>

          <div className="flex min-h-5 flex-wrap items-center gap-1.5">
            {!item.tracked && <Tag tone="muted">Prism instance</Tag>}
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
