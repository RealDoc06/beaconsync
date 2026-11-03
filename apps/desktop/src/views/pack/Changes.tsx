import { useState } from 'react'
import { Button, Empty, ErrorLine } from '../../components/ui'
import { call } from '../../lib/api'
import { loaderText } from '../../lib/format'
import { askConfirm, packKeys, useAction } from '../../lib/store'
import type { Change, ChangeKind } from '../../lib/types'
import { useStatus } from '../Pack'

const KIND: Record<ChangeKind, { label: string; color: string }> = {
  added: { label: 'Added', color: 'text-ok' },
  removed: { label: 'Removed', color: 'text-bad' },
  modified: { label: 'Changed', color: 'text-warn' },
  enabled: { label: 'Turned on', color: 'text-ok' },
  disabled: { label: 'Turned off', color: 'text-muted' },
}

const GROUPS = [
  { key: 'mods', label: 'Mods', test: (c: Change) => c.path.startsWith('mods/') },
  { key: 'config', label: 'Config', test: (c: Change) => /^(config|defaultconfigs)\//.test(c.path) },
  { key: 'other', label: 'Other files', test: () => true },
] as const

function groupChanges(files: Change[]) {
  const out = GROUPS.map((g) => ({ ...g, items: [] as Change[] }))
  for (const c of files) out.find((g) => g.test(c))?.items.push(c)
  return out.filter((g) => g.items.length > 0)
}

export function ChangesTab({ path }: { path: string }) {
  const status = useStatus(path)
  const [message, setMessage] = useState('')
  const save = useAction(() => call('save_version', { path, message }), {
    label: 'Saving version',
    invalidate: packKeys(path),
    done: 'Version saved',
    onSuccess: () => setMessage(''),
  })
  const undo = useAction((ids: string[] | null) => call('discard', { path, ids }), {
    label: 'Undoing changes',
    invalidate: packKeys(path),
  })
  const abort = useAction(() => call('abort_merge', { path }), { label: 'Cancelling merge', invalidate: packKeys(path) })

  const s = status.data
  if (!s) return <ErrorLine error={status.error} />
  const d = s.diff
  const empty = d.files.length === 0 && !d.minecraft && !d.loader

  return (
    <div className="flex min-h-0 flex-1">
      <div className="min-w-0 flex-1 overflow-auto">
        {s.merging && (
          <div className="flex h-10 items-center gap-3 border-b border-line px-5 text-xs">
            <span className="text-warn">Merging. Save a version to finish.</span>
            <Button small onClick={() => abort.mutate(undefined)}>
              Cancel merge
            </Button>
          </div>
        )}
        {empty && !s.merging && <Empty>No unsaved changes</Empty>}
        {(d.minecraft || d.loader) && (
          <div className="flex flex-col gap-1 border-b border-line px-5 py-3">
            {d.minecraft && (
              <span>
                Minecraft {d.minecraft[0]} → <span className="text-warn">{d.minecraft[1]}</span>
              </span>
            )}
            {d.loader && (
              <span>
                {loaderText(d.loader[0])} → <span className="text-warn">{loaderText(d.loader[1])}</span>
              </span>
            )}
          </div>
        )}
        {groupChanges(d.files).map((g) => (
          <section key={g.key}>
            <h3 className="flex h-9 items-center border-b border-line px-5 font-semibold">
              {g.label} <span className="ml-2 font-normal text-[#666] tabular-nums">{g.items.length}</span>
            </h3>
            {g.items.map((c) => (
              <ChangeRow key={c.id} c={c} onUndo={() => undo.mutate([c.id])} busy={undo.isPending} />
            ))}
          </section>
        ))}
      </div>
      <aside className="flex w-72 shrink-0 flex-col gap-3 border-l border-line p-4">
        <textarea
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          placeholder="What changed? (optional)"
          rows={4}
          className="resize-none rounded-md border border-line-strong bg-bg p-2.5 outline-none placeholder:text-[#555] focus:border-[#666]"
        />
        <Button variant="primary" disabled={(empty && !s.merging) || save.isPending} onClick={() => save.mutate(undefined)}>
          Save version
        </Button>
        <Button
          variant="danger"
          disabled={empty || undo.isPending}
          onClick={() =>
            askConfirm({
              title: 'Undo all changes?',
              body: 'Every file goes back to the last saved version. This cannot be undone.',
              action: 'Undo all',
              danger: true,
              onConfirm: () => undo.mutate(null),
            })
          }
        >
          Undo all
        </Button>
      </aside>
    </div>
  )
}

function ChangeRow({ c, onUndo, busy }: { c: Change; onUndo: () => void; busy: boolean }) {
  const k = KIND[c.kind]
  const name = c.new?.name ?? c.old?.name ?? c.path
  const from = c.old?.version
  const to = c.new?.version
  return (
    <div className="grid h-9 grid-cols-[80px_minmax(0,1fr)_minmax(0,220px)_60px] items-center gap-3 border-b border-line px-5 hover:bg-hover">
      <span className={`text-xs ${k.color}`}>{k.label}</span>
      <span className="truncate" title={c.path}>
        {name}
        {name !== c.path && <span className="ml-2 text-xs text-[#555]">{c.path}</span>}
      </span>
      <span className="truncate text-xs text-muted">{c.kind === 'modified' && from && to && from !== to ? `${from} → ${to}` : (to ?? from ?? '')}</span>
      <Button small variant="ghost" onClick={onUndo} disabled={busy}>
        Undo
      </Button>
    </div>
  )
}
