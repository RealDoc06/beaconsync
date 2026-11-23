import { useQuery } from '@tanstack/react-query'
import { memo, useMemo, useState } from 'react'
import {
  CloseIcon,
  CubeIcon,
  GitBranchIcon,
  PlusIcon,
  RefreshIcon,
  SearchIcon,
  SparklesIcon,
} from '../../components/Icons'
import { Button, Empty, ErrorLine, Input, Segmented, Select, Tag, Toggle } from '../../components/ui'
import { call } from '../../lib/api'
import { askConfirm, keys, packKeys, useAction, useApp } from '../../lib/store'
import type { ContentFolder, FileRow, ModUpdate, PackInfo, Side } from '../../lib/types'
import { AddMods } from './AddMods'

const SIDES = [
  { value: 'both', label: 'Both sides' },
  { value: 'client', label: 'Client' },
  { value: 'server', label: 'Server' },
] as const satisfies readonly { value: Side; label: string }[]

const FOLDERS: Record<ContentFolder, string> = { mods: 'Mods', resourcepacks: 'Resource packs', shaderpacks: 'Shaders' }

export function ModsTab({ p }: { p: PackInfo }) {
  const path = p.path
  const readOnly = p.config.role === 'subscriber'
  const [folder, setFolder] = useState<ContentFolder>('mods')
  const [filter, setFilter] = useState('')
  const [adding, setAdding] = useState(false)
  const [updates, setUpdates] = useState<ModUpdate[] | null>(null)

  const files = useQuery({ queryKey: [...keys.pack(path), 'files'], queryFn: () => call('pack_files', { path }) })
  const check = useAction(() => call('check_updates', { path }), {
    label: 'Checking for updates',
    done: (u) => (u.length ? null : 'Everything is up to date'),
    onSuccess: setUpdates,
  })
  const apply = useAction((list: ModUpdate[]) => call('apply_updates', { path, updates: list }), {
    label: 'Updating',
    invalidate: packKeys(path),
    done: 'Updated. Save a version to keep it.',
    onSuccess: (_, done) => setUpdates((cur) => cur && cur.filter((u) => !done.includes(u))),
  })
  const updateFor = useMemo(() => new Map((updates ?? []).map((u) => [u.path, u])), [updates])

  const all = files.data ?? []
  const counts = { mods: 0, resourcepacks: 0, shaderpacks: 0 }
  for (const f of all) counts[f.folder]++
  const q = filter.trim().toLowerCase()
  const rows = all.filter((f) => f.folder === folder && (!q || f.name.toLowerCase().includes(q) || f.path.toLowerCase().includes(q)))

  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex flex-wrap items-center gap-2.5 border-b border-line px-6 py-3">
          <Segmented
            value={folder}
            onChange={setFolder}
            options={(Object.keys(FOLDERS) as ContentFolder[]).map((f) => ({
              value: f,
              label: (
                <>
                  {FOLDERS[f]}
                  <span className="font-mono text-[11px] text-subtle tabular-nums">{counts[f]}</span>
                </>
              ),
            }))}
          />
          <Input
            icon={<SearchIcon className="size-3.5" />}
            placeholder="Filter"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            className="w-48"
          />
          <span className="flex-1" />
          {!readOnly && (
            <>
              <Button small onClick={() => check.mutate(undefined)} disabled={check.isPending}>
                <RefreshIcon className="size-3.5" />
                Check updates
              </Button>
              <Button small variant={adding ? 'default' : 'primary'} onClick={() => setAdding(!adding)}>
                {adding ? <CloseIcon className="size-3.5" /> : <PlusIcon className="size-3.5" />}
                {adding ? 'Done adding' : 'Add mods'}
              </Button>
            </>
          )}
        </div>

        {!readOnly && p.config.upstream && <UpstreamLine path={path} name={p.config.upstream.name} />}

        {updates && updates.length > 0 && (
          <div className="flex h-10 shrink-0 items-center gap-3 border-b border-accent-border bg-accent-soft px-6 text-xs text-accent-text">
            <SparklesIcon className="size-3.5 shrink-0" />
            <span className="font-medium">
              {updates.length} mod update{updates.length > 1 ? 's' : ''} available
            </span>
            <Button small variant="primary" className="ml-auto" disabled={apply.isPending} onClick={() => apply.mutate(updates)}>
              Update all
            </Button>
          </div>
        )}

        <div className="@container min-h-0 flex-1 overflow-auto">
          {files.error && (
            <div className="p-6 pb-0">
              <ErrorLine error={files.error} />
            </div>
          )}
          {files.data && rows.length === 0 && (
            <Empty
              icon={<CubeIcon className="size-5" />}
              title={q ? 'No matching files' : `No ${FOLDERS[folder].toLowerCase()} yet`}
              action={
                !readOnly && !adding && !q ? (
                  <Button variant="primary" onClick={() => setAdding(true)}>
                    <PlusIcon className="size-3.5" />
                    Add mods
                  </Button>
                ) : undefined
              }
            >
              {q ? 'Try a different filter term.' : 'Search Modrinth to add content compatible with this pack.'}
            </Empty>
          )}
          {rows.map((f) => (
            <ModRow
              key={f.path}
              path={path}
              file={f}
              readOnly={readOnly}
              update={updateFor.get(f.path)}
              onUpdate={(u) => apply.mutate([u])}
              updating={apply.isPending}
            />
          ))}
        </div>
      </div>
      {adding && <AddMods p={p} installed={all} />}
    </div>
  )
}

const ModRow = memo(function ModRow({
  path,
  file,
  readOnly,
  update,
  onUpdate,
  updating,
}: {
  path: string
  file: FileRow
  readOnly: boolean
  update: ModUpdate | undefined
  onUpdate: (u: ModUpdate) => void
  updating: boolean
}) {
  const set = useApp((s) => s.set)
  const toggle = useAction((enabled: boolean) => call('set_enabled', { path, file: file.path, enabled }), {
    label: file.enabled ? 'Disabling' : 'Enabling',
    invalidate: packKeys(path),
  })
  const side = useAction((s: Side) => call('set_side', { path, file: file.path, side: s }), {
    label: 'Changing side',
    invalidate: packKeys(path),
  })
  const remove = useAction(() => call('remove_files', { path, files: [file.path] }), {
    label: `Removing ${file.name}`,
    invalidate: packKeys(path),
    onSuccess: () => set({ notice: { kind: 'ok', text: `Removed ${file.name}. Undo it from Changes if needed.` } }),
  })
  const filename = file.path.split('/').pop()
  return (
    <div
      className={`group grid min-h-11.5 grid-cols-[36px_minmax(0,1fr)_minmax(0,140px)_112px_auto] items-center @max-[720px]:grid-cols-[36px_minmax(0,1fr)_104px_auto] @max-[480px]:grid-cols-[36px_minmax(0,1fr)_auto] gap-3 border-b border-line px-6 py-1.5 transition-colors hover:bg-hover ${
        file.enabled ? '' : 'opacity-50'
      }`}
    >
      <Toggle
        checked={file.enabled}
        onChange={(v) => toggle.mutate(v)}
        disabled={readOnly || toggle.isPending}
        ariaLabel={`Enable ${file.name}`}
      />
      <div className="min-w-0 truncate" title={filename}>
        <span className="font-medium text-fg">{file.name}</span>
        {filename && <span className="ml-2 font-mono text-[11px] text-subtle @max-[480px]:hidden">{filename}</span>}
      </div>
      <span className="truncate font-mono text-[11.5px] text-muted @max-[720px]:hidden" title={file.version ?? ''}>
        {file.version}
      </span>
      <div className="@max-[480px]:hidden">
        {readOnly ? (
          <Tag tone="muted" className="justify-self-start">
            {SIDES.find((s) => s.value === file.side)?.label}
          </Tag>
        ) : (
          <Select
            aria-label={`Side for ${file.name}`}
            size="sm"
            value={file.side}
            onChange={(s) => side.mutate(s)}
            options={SIDES}
            className="w-full"
          />
        )}
      </div>
      <div className="flex justify-end gap-1">
        {update && (
          <Button
            small
            variant="primary"
            disabled={updating}
            onClick={() => onUpdate(update)}
            title={`${update.current ?? ''} → ${update.entry.version ?? ''}`}
          >
            Update
          </Button>
        )}
        {!readOnly && (
          <Button
            size="icon"
            variant="ghost-danger"
            className="opacity-65 group-hover:opacity-100 focus-visible:opacity-100"
            aria-label={`Remove ${file.name}`}
            title="Remove"
            onClick={() =>
              askConfirm({
                title: `Remove ${file.name}?`,
                body: 'The file is deleted from the pack folder.',
                action: 'Remove',
                danger: true,
                onConfirm: () => remove.mutate(undefined),
              })
            }
          >
            <CloseIcon className="size-3.5" />
          </Button>
        )}
      </div>
    </div>
  )
})

/** "Get updates from <upstream>" for forks and imported modpacks. */
function UpstreamLine({ path, name }: { path: string; name: string }) {
  const set = useApp((s) => s.set)
  const info = useQuery({
    queryKey: [...keys.pack(path), 'upstream'],
    queryFn: () => call('check_upstream', { path }),
    // Upstream releases should show up within a minute without a restart.
    staleTime: 30_000,
    refetchInterval: 60_000,
  })
  const pull = useAction(() => call('pull_upstream', { path }), {
    label: `Getting updates from ${name}`,
    invalidate: packKeys(path),
    done: (c) => (c.length ? null : `Got updates from ${name}. Save a version to keep them.`),
    onSuccess: (conflicts) => conflicts.length && set({ conflicts: { path, items: conflicts } }),
  })
  const u = info.data
  return (
    <div
      className={`flex h-10 shrink-0 items-center gap-2.5 border-b px-6 text-xs ${
        u?.available ? 'border-info-border bg-info-soft text-info' : 'border-line bg-sidebar/40 text-muted'
      }`}
    >
      <GitBranchIcon className="size-3.5 shrink-0" />
      <span>
        Based on <span className="font-medium text-fg-secondary">{name}</span>
      </span>
      {u?.available ? (
        <Button small variant="primary" className="ml-auto" onClick={() => pull.mutate(undefined)} disabled={pull.isPending}>
          Get updates ({u.version})
        </Button>
      ) : (
        <span className="text-subtle">{u ? `Up to date (${u.version})` : info.error ? 'Could not check' : ''}</span>
      )}
    </div>
  )
}
