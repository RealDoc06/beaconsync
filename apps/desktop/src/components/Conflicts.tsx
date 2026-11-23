import { useState } from 'react'
import { call } from '../lib/api'
import { packKeys, useAction, useApp } from '../lib/store'
import type { Conflict, ConflictSide, Pick } from '../lib/types'
import { CheckIcon } from './Icons'
import { Button, Dialog } from './ui'

/** Picker for merge conflicts (branch merges and upstream updates): keep mine or take theirs. */
export function Conflicts() {
  const conflicts = useApp((s) => s.conflicts)
  const set = useApp((s) => s.set)
  if (!conflicts) return null
  return <ConflictDialog path={conflicts.path} items={conflicts.items} onClose={() => set({ conflicts: null })} />
}

const describe = (side: ConflictSide) => {
  if (!side) return 'Removed'
  const [, e] = side
  // Mods have a name and version; plain files only differ in content.
  if (e.name) return e.version ? `${e.name} ${e.version}` : e.name
  return e.size < 1024 ? `${e.size} bytes` : `${(e.size / 1024).toFixed(1)} KB`
}

function ConflictDialog({ path, items, onClose }: { path: string; items: Conflict[]; onClose: () => void }) {
  const [picks, setPicks] = useState<Record<string, Pick>>(() => Object.fromEntries(items.map((c) => [c.id, 'theirs'])))
  const apply = useAction(
    () => call('resolve_conflicts', { path, conflicts: items, picks: Object.entries(picks) as [string, Pick][] }),
    { label: 'Applying choices', invalidate: packKeys(path), done: 'Choices applied. Save a version to finish.', onSuccess: onClose },
  )
  const option = (c: Conflict, pick: Pick, side: ConflictSide) => {
    const selected = picks[c.id] === pick
    return (
      <button
        type="button"
        onClick={() => setPicks({ ...picks, [c.id]: pick })}
        className={`flex min-w-0 flex-col gap-0.5 rounded-xl border px-3 py-2 text-left transition-all ${
          selected
            ? 'border-accent bg-accent-soft text-fg shadow-theme-xs'
            : 'border-line-strong bg-bg/60 text-muted hover:border-line-hover hover:text-fg-secondary'
        }`}
      >
        <span className="flex items-center justify-between gap-1 text-[11px] font-medium">
          <span className={selected ? 'text-accent-text' : 'text-muted'}>{pick === 'ours' ? 'Keep mine' : 'Take theirs'}</span>
          {selected && <CheckIcon className="size-3 text-accent-text" />}
        </span>
        <span className="truncate text-xs font-medium">{describe(side)}</span>
      </button>
    )
  }
  return (
    <Dialog
      wide
      title={`${items.length} file${items.length === 1 ? '' : 's'} changed on both sides`}
      subtitle="Choose which version to keep for each conflicting file."
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Keep mine for all</Button>
          <Button variant="primary" onClick={() => apply.mutate(undefined)} disabled={apply.isPending}>
            Apply choices
          </Button>
        </>
      }
    >
      <div className="flex flex-col divide-y divide-line">
        {items.map((c) => (
          <div key={c.id} className="grid grid-cols-[minmax(0,1fr)_210px_210px] items-center gap-3 py-2.5 first:pt-0 last:pb-0">
            <span className="truncate font-mono text-xs text-fg-secondary" title={c.id}>
              {(c.ours ?? c.theirs ?? c.base)?.[0] ?? c.id}
            </span>
            {option(c, 'ours', c.ours)}
            {option(c, 'theirs', c.theirs)}
          </div>
        ))}
      </div>
    </Dialog>
  )
}
