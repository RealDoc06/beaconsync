import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { GitBranchIcon, GitMergeIcon, HistoryIcon, PlusIcon, TrashIcon, UndoIcon } from '../../components/Icons'
import { Button, Dialog, Empty, ErrorLine, Input, Select, Tag, Toggle } from '../../components/ui'
import { call } from '../../lib/api'
import { ago, shortId } from '../../lib/format'
import { askConfirm, keys, packKeys, useAction, useApp } from '../../lib/store'
import type { History, LogEntry } from '../../lib/types'

export function HistoryTab({ path, published }: { path: string; published: boolean }) {
  const history = useQuery({ queryKey: [...keys.pack(path), 'history'], queryFn: () => call('history', { path }) })
  const [rollingBack, setRollingBack] = useState<LogEntry | null>(null)
  const reset = useAction((id: string) => call('reset_version', { path, id }), {
    label: 'Going back',
    invalidate: packKeys(path),
    done: 'Went back to the older version',
  })
  const h = history.data
  if (!h) {
    return history.error ? (
      <div className="p-6">
        <ErrorLine error={history.error} />
      </div>
    ) : null
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <Branches path={path} h={h} />
      <div className="@container min-h-0 flex-1 overflow-auto">
        {h.log.length === 0 ? (
          <Empty icon={<HistoryIcon className="size-5" />} title="No saved versions yet">
            Make edits in Mods or configs, then save your first snapshot in the Changes tab.
          </Empty>
        ) : (
          <div className="flex flex-col p-6">
            <div className="overflow-hidden rounded-2xl border border-line bg-surface shadow-theme-xs">
              {h.log.map((e, i) => (
                <VersionRow
                  key={e.id}
                  e={e}
                  current={e.id === h.head}
                  first={i === 0}
                  last={i === h.log.length - 1}
                  newer={i}
                  onRestore={() => setRollingBack(e)}
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
        )}
      </div>
      {rollingBack && <RollbackDialog path={path} e={rollingBack} published={published} onClose={() => setRollingBack(null)} />}
    </div>
  )
}

/** Rolls back by saving the old files as a new version, and for published packs also
 * releasing and deploying it so players and servers go back too. */
function RollbackDialog({ path, e, published, onClose }: { path: string; e: LogEntry; published: boolean; onClose: () => void }) {
  const [everyone, setEveryone] = useState(published)
  const rollback = useAction(() => call('rollback', { path, id: e.id, publish: everyone }), {
    label: 'Rolling back',
    invalidate: packKeys(path),
    done: (r) =>
      r.release
        ? `Rolled back. Players get ${r.release.name}${r.servers ? `, ${r.servers} server${r.servers === 1 ? '' : 's'} updating` : ''}`
        : 'Rolled back and saved as a new version',
    onSuccess: onClose,
  })
  return (
    <Dialog
      title="Roll back to this version?"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={rollback.isPending} onClick={() => rollback.mutate(undefined)}>
            Roll back
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <p className="leading-relaxed text-fg-secondary">
          Files go back to <span className="font-semibold text-fg">{e.message || shortId(e.id)}</span> and that is saved as a
          new version. Unsaved changes are replaced. Nothing in History is lost.
        </p>
        {published && <Toggle checked={everyone} onChange={setEveryone} label="Also roll back players and servers" />}
        {published && everyone && (
          <p className="rounded-lg border border-accent-border bg-accent-soft px-3 py-2 text-xs text-accent-text">
            Publishes this rollback as an update and deploys it to all connected servers.
          </p>
        )}
      </div>
    </Dialog>
  )
}

function VersionRow({
  e,
  current,
  first,
  last,
  newer,
  onRestore,
  onReset,
}: {
  e: LogEntry
  current: boolean
  first: boolean
  last: boolean
  newer: number
  onRestore: () => void
  onReset: () => void
}) {
  return (
    <div className="group grid min-h-13 grid-cols-[24px_minmax(0,1fr)_96px_156px_174px] @max-[720px]:grid-cols-[24px_minmax(0,1fr)_148px_166px] items-center gap-3 border-b border-line px-4 last:border-b-0 transition-colors hover:bg-hover">
      {/* Vertical timeline spine + node */}
      <div className="relative flex h-full items-center justify-center self-stretch">
        {!first && <span className="absolute top-0 bottom-1/2 w-px bg-line-strong" />}
        {!last && <span className="absolute top-1/2 bottom-0 w-px bg-line-strong" />}
        <span
          className={`relative z-10 rounded-full ${
            current ? 'size-2.5 bg-accent ring-4 ring-accent-soft' : 'size-2 border border-line-hover bg-surface'
          }`}
        />
      </div>

      <div className="flex min-w-0 items-center gap-2 py-2.5">
        <span className={`truncate ${current ? 'font-semibold text-fg' : 'font-medium text-fg-secondary'}`}>
          {e.message || 'Untitled version'}
        </span>
        {current && (
          <Tag tone="accent" dot>
            Current
          </Tag>
        )}
        {e.parents.length > 1 && (
          <Tag tone="info">
            <GitMergeIcon className="size-3" />
            Merge
          </Tag>
        )}
      </div>

      <span className="truncate text-xs text-muted @max-[720px]:hidden">{e.author}</span>

      <span
        className="flex items-center justify-end gap-1.5 whitespace-nowrap text-xs text-muted"
        title={new Date(e.time * 1000).toLocaleString()}
      >
        <span className="truncate">{ago(e.time)}</span>
        <span className="rounded bg-surface-2 px-1.5 py-0.5 font-mono text-[10.5px] text-fg-secondary">{shortId(e.id)}</span>
      </span>

      <div className="flex justify-end gap-1 opacity-65 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
        {!current && (
          <>
            <Button small onClick={onRestore}>
              <UndoIcon className="size-3" />
              Roll back
            </Button>
            {newer > 0 && (
              <Button small variant="ghost-danger" onClick={onReset}>
                Reset
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
    <div className="flex flex-wrap items-center gap-2 border-b border-line px-6 py-3">
      <span className="inline-flex items-center gap-1.5 text-xs font-medium text-muted">
        <GitBranchIcon className="size-3.5 text-accent-text" />
        Branch
      </span>
      <Select
        aria-label="Current branch"
        size="sm"
        value={current || '(detached)'}
        onChange={(b) => switchTo.mutate(b)}
        options={
          current
            ? h.branches.map((b) => ({ value: b.name, label: b.name }))
            : [{ value: '(detached)', label: 'Old version' }, ...h.branches.map((b) => ({ value: b.name, label: b.name }))]
        }
      />
      <Button small onClick={() => setCreating(true)}>
        <PlusIcon className="size-3" />
        New branch
      </Button>
      {others.length > 0 && (
        <>
          <span className="mx-1 h-4 border-l border-line-strong" />
          <Select
            aria-label="Branch to merge or delete"
            size="sm"
            value={from}
            onChange={setMergeFrom}
            options={others.map((b) => ({ value: b.name, label: b.name }))}
          />
          <Button small variant="ghost" disabled={!current} onClick={() => merge.mutate(from)}>
            <GitMergeIcon className="size-3" />
            Merge into {current || 'current'}
          </Button>
          <Button
            small
            variant="ghost-danger"
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
            <TrashIcon className="size-3" />
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
            Create branch
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-2.5">
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          className="w-full font-mono"
          placeholder="experiments"
          onKeyDown={(e) => e.key === 'Enter' && valid && create.mutate(undefined)}
        />
        <p className="text-xs leading-relaxed text-muted">
          A branch lets you try changes without touching the main pack. It starts from the current version.
        </p>
      </div>
    </Dialog>
  )
}
