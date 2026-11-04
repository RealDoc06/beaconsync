import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Button, Dialog, Empty, ErrorLine, Input, Select } from '../../components/ui'
import { call } from '../../lib/api'
import { ago, shortId } from '../../lib/format'
import { askConfirm, keys, packKeys, useAction, useApp } from '../../lib/store'
import type { History, LogEntry } from '../../lib/types'

export function HistoryTab({ path }: { path: string }) {
  const history = useQuery({ queryKey: [...keys.pack(path), 'history'], queryFn: () => call('history', { path }) })
  const restore = useAction((id: string) => call('restore_version', { path, id }), {
    label: 'Restoring version',
    invalidate: packKeys(path),
    done: 'Restored and saved as a new version',
  })
  const reset = useAction((id: string) => call('reset_version', { path, id }), {
    label: 'Going back',
    invalidate: packKeys(path),
    done: 'Went back to the older version',
  })
  const h = history.data
  if (!h) return <ErrorLine error={history.error} />

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <Branches path={path} h={h} />
      <div className="min-h-0 flex-1 overflow-auto border-t border-line">
        {h.log.length === 0 && <Empty>No saved versions yet</Empty>}
        {h.log.map((e, i) => (
          <VersionRow
            key={e.id}
            e={e}
            current={e.id === h.head}
            newer={i}
            onRestore={() =>
              askConfirm({
                title: 'Restore this version?',
                body: `Files go back to "${e.message}" and that is saved as a new version. Unsaved changes are replaced. Nothing in History is lost.`,
                action: 'Restore',
                onConfirm: () => restore.mutate(e.id),
              })
            }
            onReset={() =>
              askConfirm({
                title: 'Go back to this version?',
                body: `The ${i} newer version${i === 1 ? '' : 's'} on this branch will be removed from History, and unsaved changes are replaced.`,
                action: 'Go back',
                danger: true,
                onConfirm: () => reset.mutate(e.id),
              })
            }
          />
        ))}
      </div>
    </div>
  )
}

function VersionRow({
  e,
  current,
  newer,
  onRestore,
  onReset,
}: {
  e: LogEntry
  current: boolean
  newer: number
  onRestore: () => void
  onReset: () => void
}) {
  return (
    <div className="group grid h-11 grid-cols-[minmax(0,1fr)_100px_160px_300px] items-center gap-3 border-b border-line px-5 hover:bg-hover">
      <div className="flex min-w-0 items-center gap-2">
        <span className={`size-1.5 shrink-0 rounded-full ${current ? 'bg-accent' : 'bg-[#333]'}`} />
        <span className="truncate">{e.message || 'Untitled version'}</span>
        {current && <span className="text-xs text-accent">Current</span>}
        {e.parents.length > 1 && <span className="text-xs text-muted">merge</span>}
      </div>
      <span className="truncate text-xs text-muted">{e.author}</span>
      <span className="whitespace-nowrap text-xs text-muted" title={new Date(e.time * 1000).toLocaleString()}>
        {ago(e.time)} · <span className="font-mono">{shortId(e.id)}</span>
      </span>
      <div className="flex justify-end gap-1 opacity-0 group-hover:opacity-100 focus-within:opacity-100">
        {!current && (
          <>
            <Button small onClick={onRestore}>
              Restore this version
            </Button>
            {newer > 0 && (
              <Button small variant="ghost" onClick={onReset}>
                Go back to this version
              </Button>
            )}
          </>
        )}
      </div>
    </div>
  )
}

/** Branches stay tucked here: switch, create, merge into current, delete. */
function Branches({ path, h }: { path: string; h: History }) {
  const set = useApp((s) => s.set)
  const [creating, setCreating] = useState(false)
  const current = h.branches.find((b) => b.current)?.name ?? ''
  const others = h.branches.filter((b) => !b.current)
  const [mergeFrom, setMergeFrom] = useState('')
  const from = others.some((b) => b.name === mergeFrom) ? mergeFrom : (others[0]?.name ?? '')

  const switchTo = useAction((name: string) => call('switch_branch', { path, name }), {
    label: 'Switching branch',
    invalidate: packKeys(path),
  })
  const merge = useAction((name: string) => call('merge_branch', { path, name }), {
    label: 'Merging',
    invalidate: packKeys(path),
    done: (r) => (r.fast_forward ? `Moved ${current} forward to ${from}` : r.conflicts.length ? null : `Merged ${from}. Save a version to finish.`),
    onSuccess: (r) => r.conflicts.length && set({ conflicts: { path, items: r.conflicts } }),
  })
  const remove = useAction((name: string) => call('delete_branch', { path, name }), {
    label: 'Deleting branch',
    invalidate: packKeys(path),
  })

  return (
    <div className="flex h-12 shrink-0 items-center gap-2 px-5">
      <span className="text-xs text-muted">Branch</span>
      <Select
        value={current || '(detached)'}
        onChange={(b) => switchTo.mutate(b)}
        options={current ? h.branches.map((b) => ({ value: b.name, label: b.name })) : [{ value: '(detached)', label: 'Old version' }, ...h.branches.map((b) => ({ value: b.name, label: b.name }))]}
        className="h-7 text-xs"
      />
      <Button small variant="ghost" onClick={() => setCreating(true)}>
        New branch
      </Button>
      {others.length > 0 && (
        <>
          <span className="mx-2 h-4 border-l border-line" />
          <Select value={from} onChange={setMergeFrom} options={others.map((b) => ({ value: b.name, label: b.name }))} className="h-7 text-xs" />
          <Button small variant="ghost" disabled={!current} onClick={() => merge.mutate(from)}>
            Merge into {current || 'current'}
          </Button>
          <Button
            small
            variant="ghost"
            onClick={() =>
              askConfirm({
                title: `Delete branch ${from}?`,
                body: 'Versions only on this branch will no longer be shown.',
                action: 'Delete',
                danger: true,
                onConfirm: () => remove.mutate(from),
              })
            }
          >
            Delete {from}
          </Button>
        </>
      )}
      {creating && <NewBranch path={path} onClose={() => setCreating(false)} />}
    </div>
  )
}

function NewBranch({ path, onClose }: { path: string; onClose: () => void }) {
  const [name, setName] = useState('')
  const create = useAction(() => call('create_branch', { path, name }), {
    label: 'Creating branch',
    invalidate: packKeys(path),
    onSuccess: onClose,
  })
  const valid = /^[\w.-]+$/.test(name)
  return (
    <Dialog
      title="New branch"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!valid} onClick={() => create.mutate(undefined)}>
            Create
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-2">
        <Input value={name} onChange={(e) => setName(e.target.value)} className="w-full" placeholder="experiments" onKeyDown={(e) => e.key === 'Enter' && valid && create.mutate(undefined)} />
        <p className="text-xs text-muted">A branch lets you try changes without touching the main pack. It starts from the current version.</p>
      </div>
    </Dialog>
  )
}
