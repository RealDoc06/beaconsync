import { useState } from 'react'
import { CheckIcon, DiffIcon, EyeOffIcon, GitMergeIcon, UndoIcon } from '../../components/Icons'
import { Button, Empty, ErrorLine, Tag, Textarea, type TagTone } from '../../components/ui'
import { call } from '../../lib/api'
import { loaderText } from '../../lib/format'
import { askConfirm, packKeys, useAction } from '../../lib/store'
import type { Change, ChangeKind } from '../../lib/types'
import { useStatus } from '../Pack'

const KIND: Record<ChangeKind, { label: string; tone: TagTone }> = {
  added: { label: 'Added', tone: 'ok' },
  removed: { label: 'Removed', tone: 'bad' },
  modified: { label: 'Changed', tone: 'warn' },
  enabled: { label: 'Turned on', tone: 'ok' },
  disabled: { label: 'Turned off', tone: 'muted' },
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
  const ignore = useAction((file: string) => call('ignore_files', { path, files: [file] }), {
    label: 'Ignoring file',
    invalidate: packKeys(path),
  })
  const abort = useAction(() => call('abort_merge', { path }), { label: 'Cancelling merge', invalidate: packKeys(path) })

  const s = status.data
  if (!s) {
    return status.error ? (
      <div className="p-6">
        <ErrorLine error={status.error} />
      </div>
    ) : null
  }
  const d = s.diff
  const empty = d.files.length === 0 && !d.minecraft && !d.loader
  const totalChanges = d.files.length + (d.minecraft ? 1 : 0) + (d.loader ? 1 : 0)

  return (
    <div className="flex min-h-0 flex-1">
      <div className="@container min-w-0 flex-1 overflow-auto">
        {s.merging && (
          <div className="flex h-11 items-center gap-3 border-b border-warn-border bg-warn-soft px-6 text-xs text-warn">
            <GitMergeIcon className="size-4 shrink-0" />
            <span className="font-medium">Merge in progress. Save a version to complete it.</span>
            <Button small className="ml-auto" onClick={() => abort.mutate(undefined)}>
              Cancel merge
            </Button>
          </div>
        )}

        {empty && !s.merging && (
          <Empty icon={<CheckIcon className="size-5 text-ok" />} title="Working tree clean">
            All mods and configs match your last saved version.
          </Empty>
        )}

        {!empty && (
          <div className="flex flex-col gap-4 p-6">
            {(d.minecraft || d.loader) && (
              <div className="flex flex-col gap-1.5 rounded-xl border border-warn-border bg-warn-soft px-4 py-3 text-xs">
                {d.minecraft && (
                  <span className="text-fg">
                    Minecraft <span className="font-mono text-muted">{d.minecraft[0]}</span> →{' '}
                    <span className="font-mono font-semibold text-warn">{d.minecraft[1]}</span>
                  </span>
                )}
                {d.loader && (
                  <span className="text-fg">
                    Loader <span className="text-muted">{loaderText(d.loader[0])}</span> →{' '}
                    <span className="font-semibold text-warn">{loaderText(d.loader[1])}</span>
                  </span>
                )}
              </div>
            )}

            {groupChanges(d.files).map((g) => (
              <section key={g.key} className="overflow-hidden rounded-2xl border border-line bg-surface shadow-theme-xs">
                <h3 className="flex h-9.5 items-center justify-between border-b border-line bg-sidebar/60 px-4 text-xs font-semibold text-fg">
                  <span>{g.label}</span>
                  <span className="rounded-full bg-surface-2 px-2 py-0.5 font-mono text-[10.5px] font-medium text-muted tabular-nums">
                    {g.items.length}
                  </span>
                </h3>
                <div className="divide-y divide-line">
                  {g.items.map((c) => (
                    <ChangeRow
                      key={c.id}
                      c={c}
                      onUndo={() => undo.mutate([c.id])}
                      // Ignoring a mod would drop it from the pack, and the ignore list must reach players.
                      onIgnore={
                        g.key === 'mods' || c.path === '.beaconignore'
                          ? undefined
                          : () =>
                              askConfirm({
                                title: 'Ignore this file?',
                                body: `Changes to ${c.path} stop showing up and new versions leave it out. Your copy stays as it is.`,
                                action: 'Ignore',
                                onConfirm: () => ignore.mutate(c.path),
                              })
                      }
                      busy={undo.isPending || ignore.isPending}
                    />
                  ))}
                </div>
              </section>
            ))}
          </div>
        )}
      </div>

      <aside className="flex w-64 lg:w-76 shrink-0 flex-col gap-3.5 border-l border-line bg-sidebar p-5">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2 text-xs font-semibold text-fg">
            <DiffIcon className="size-3.5 text-accent-text" />
            Save snapshot
          </div>
          {totalChanges > 0 && (
            <Tag tone="warn" dot>
              {totalChanges} change{totalChanges === 1 ? '' : 's'}
            </Tag>
          )}
        </div>

        <Textarea
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          placeholder="What changed? (optional)"
          rows={5}
          className="resize-none"
        />

        <Button
          variant="primary"
          className="w-full"
          disabled={(empty && !s.merging) || save.isPending}
          onClick={() => save.mutate(undefined)}
        >
          <CheckIcon className="size-3.5" />
          Save version
        </Button>

        <Button
          variant="danger"
          className="w-full"
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
          <UndoIcon className="size-3.5" />
          Undo all changes
        </Button>

        <p className="mt-auto text-[11.5px] leading-relaxed text-subtle">
          Versions work like git commits. You can roll back, branch, or publish any saved version at any time.
        </p>
      </aside>
    </div>
  )
}

function ChangeRow({ c, onUndo, onIgnore, busy }: { c: Change; onUndo: () => void; onIgnore?: () => void; busy: boolean }) {
  const k = KIND[c.kind]
  const name = c.new?.name ?? c.old?.name ?? c.path
  const from = c.old?.version
  const to = c.new?.version
  const version = c.kind === 'modified' && from && to && from !== to ? `${from} → ${to}` : (to ?? from)
  return (
    <div className="group grid min-h-10.5 grid-cols-[78px_minmax(0,1fr)_minmax(0,170px)_auto] @max-[560px]:grid-cols-[74px_minmax(0,1fr)_auto] items-center gap-2.5 px-4 py-1.5 transition-colors hover:bg-hover">
      <Tag tone={k.tone} className="justify-self-start">
        {k.label}
      </Tag>
      <div className={`min-w-0 ${version ? '' : 'col-span-2 @max-[560px]:col-span-1'}`} title={c.path}>
        <div className="truncate font-medium text-fg">
          {name}
          {name !== c.path && <span className="ml-2 font-mono text-[11px] font-normal text-subtle @max-[560px]:hidden">{c.path}</span>}
        </div>
        {version && <div className="hidden truncate font-mono text-[11px] text-muted @max-[560px]:block">{version}</div>}
      </div>
      {version && <span className="truncate font-mono text-[11.5px] text-muted @max-[560px]:hidden">{version}</span>}
      <div className="flex shrink-0 justify-end gap-1">
        {onIgnore && (
          <Button small variant="ghost" onClick={onIgnore} disabled={busy} title="Ignore file" aria-label={`Ignore ${name}`}>
            <EyeOffIcon className="size-3" />
            <span className="@max-[460px]:hidden">Ignore</span>
          </Button>
        )}
        <Button small variant="ghost" onClick={onUndo} disabled={busy} title="Undo change" aria-label={`Undo ${name}`}>
          <UndoIcon className="size-3" />
          <span className="@max-[460px]:hidden">Undo</span>
        </Button>
      </div>
    </div>
  )
}
