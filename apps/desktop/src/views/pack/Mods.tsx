import { useQuery } from '@tanstack/react-query'
import { memo, useMemo, useState } from 'react'
import { Button, Empty, ErrorLine, Input, Segmented, Select, Toggle } from '../../components/ui'
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
        <div className="flex flex-wrap items-center gap-2 px-5 py-3">
          <Segmented
            value={folder}
            onChange={setFolder}
            options={(Object.keys(FOLDERS) as ContentFolder[]).map((f) => ({
              value: f,
              label: (
                <>
                  {FOLDERS[f]} <span className="tabular-nums text-[#666]">{counts[f]}</span>
                </>
              ),
            }))}
          />
          <Input placeholder="Filter" value={filter} onChange={(e) => setFilter(e.target.value)} className="w-40" />
          <span className="flex-1" />
          {!readOnly && (
            <>
              <Button onClick={() => check.mutate(undefined)} disabled={check.isPending}>
                Check for updates
              </Button>
              <Button variant={adding ? 'default' : 'primary'} onClick={() => setAdding(!adding)}>
                {adding ? 'Done adding' : 'Add mods'}
              </Button>
            </>
          )}
        </div>
        {!readOnly && p.config.upstream && <UpstreamLine path={path} name={p.config.upstream.name} />}
        {updates && updates.length > 0 && (
          <div className="flex h-10 items-center gap-3 border-y border-line px-5 text-xs">
            <span className="text-accent">
              {updates.length} update{updates.length > 1 ? 's' : ''} available
            </span>
            <Button small variant="primary" disabled={apply.isPending} onClick={() => apply.mutate(updates)}>
              Update all
            </Button>
          </div>
        )}
        <div className="@container min-h-0 flex-1 overflow-auto border-t border-line">
          <ErrorLine error={files.error} />
          {files.data && rows.length === 0 && (
            <Empty action={!readOnly && !adding && <Button onClick={() => setAdding(true)}>Add mods</Button>}>
              {q ? 'Nothing matches the filter' : `No ${FOLDERS[folder].toLowerCase()} yet`}
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
    <div className={`grid h-10 grid-cols-[36px_minmax(0,1fr)_minmax(0,150px)_112px_96px] items-center @max-[700px]:grid-cols-[36px_minmax(0,1fr)_112px_96px] gap-3 border-b border-line px-5 hover:bg-hover ${file.enabled ? '' : 'opacity-50'}`}>
      <Toggle checked={file.enabled} onChange={(v) => toggle.mutate(v)} disabled={readOnly} label={undefined} />
      <div className="min-w-0 truncate" title={filename}>
        <span>{file.name}</span>
        <span className="ml-2 text-xs text-[#555]">{filename}</span>
      </div>
      <span className="truncate text-xs text-muted @max-[700px]:hidden" title={file.version ?? ''}>
        {file.version}
      </span>
      {readOnly ? (
        <span className="text-xs text-muted">{SIDES.find((s) => s.value === file.side)?.label}</span>
      ) : (
        <Select value={file.side} onChange={(s) => side.mutate(s)} options={SIDES} className="h-7 w-full text-xs" />
      )}
      <div className="flex justify-end gap-1">
        {update && (
          <Button small variant="primary" disabled={updating} onClick={() => onUpdate(update)} title={`${update.current ?? ''} → ${update.entry.version ?? ''}`}>
            Update
          </Button>
        )}
        {!readOnly && (
          <Button
            small
            variant="ghost"
            aria-label={`Remove ${file.name}`}
            title="Remove"
            onClick={() =>
              askConfirm({ title: `Remove ${file.name}?`, body: 'The file is deleted from the pack folder.', action: 'Remove', danger: true, onConfirm: () => remove.mutate(undefined) })
            }
          >
            ✕
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
    staleTime: 300_000,
  })
  const pull = useAction(() => call('pull_upstream', { path }), {
    label: `Getting updates from ${name}`,
    invalidate: packKeys(path),
    done: (c) => (c.length ? null : `Got updates from ${name}. Save a version to keep them.`),
    onSuccess: (conflicts) => conflicts.length && set({ conflicts: { path, items: conflicts } }),
  })
  const u = info.data
  return (
    <div className="flex h-10 items-center gap-3 border-t border-line px-5 text-xs">
      <span className="text-muted">Based on {name}</span>
      {u?.available ? (
        <Button small variant="primary" onClick={() => pull.mutate(undefined)} disabled={pull.isPending}>
          Get updates from {name} ({u.version})
        </Button>
      ) : (
        <span className="text-muted">{u ? `Up to date (${u.version})` : info.error ? 'Could not check' : ''}</span>
      )}
    </div>
  )
}
