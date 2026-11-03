import { useState } from 'react'
import { call } from '../lib/api'
import { packKeys, useAction, useApp } from '../lib/store'
import type { Conflict, ConflictSide, Pick } from '../lib/types'
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
  const option = (c: Conflict, pick: Pick, side: ConflictSide) => (
    <button
      type="button"
      onClick={() => setPicks({ ...picks, [c.id]: pick })}
      className={`flex min-w-0 flex-col rounded-md border px-2.5 py-1.5 text-left ${
        picks[c.id] === pick ? 'border-accent' : 'border-line-strong text-muted hover:border-[#555]'
      }`}
    >
      <span className="text-[11px] text-muted">{pick === 'ours' ? 'Keep mine' : 'Take theirs'}</span>
      <span className="truncate">{describe(side)}</span>
    </button>
  )
  return (
    <Dialog
      wide
      title={`${items.length} file${items.length === 1 ? '' : 's'} changed on both sides`}
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
      <div className="flex flex-col">
        {items.map((c) => (
          <div key={c.id} className="grid grid-cols-[minmax(0,1fr)_200px_200px] items-center gap-3 border-b border-line py-2">
            <span className="truncate text-xs" title={c.id}>
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
