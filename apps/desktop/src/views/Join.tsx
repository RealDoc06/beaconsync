import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { GitBranchIcon, LinkCodeIcon, PlayIcon, SearchIcon, ServerIcon } from '../components/Icons'
import { PageHeader } from '../components/PageHeader'
import { Button, Input, Tag } from '../components/ui'
import { call } from '../lib/api'
import { loaderText, shortId } from '../lib/format'
import { keys, useAction, useApp } from '../lib/store'
import type { JoinAs, JoinPreview } from '../lib/types'

export function JoinView({ initial }: { initial?: string }) {
  const go = useApp((s) => s.go)
  const [input, setInput] = useState(initial ?? '')
  const [preview, setPreview] = useState<JoinPreview | null>(null)
  const lookup = useAction((code: string) => call('preview_code', { input: code }), {
    label: 'Looking up code',
    onSuccess: setPreview,
  })
  const join = useAction((how: JoinAs) => call('join_pack', { input, how }), {
    label: 'Downloading pack',
    invalidate: [keys.library],
    done: 'Pack ready',
    onSuccess: (path) => go({ name: 'pack', path, tab: 'mods' }),
  })
  const settings = useQuery({ queryKey: keys.settings, queryFn: () => call('get_settings') })
  const p = preview?.pack
  // The owner on another machine can take an owner copy instead of following or forking.
  const s = settings.data
  const sameHub = !!preview && !!s && preview.hub.replace(/\/+$/, '') === s.hub.replace(/\/+$/, '')
  const mine = !!p && sameHub && !!s?.signed_in && p.owner === s.username

  return (
    <>
      <PageHeader title="Join with code" subtitle="Enter a friend's share code or link to play or fork their pack" />
      <div className="min-h-0 flex-1 overflow-auto">
        <div className="mx-auto flex max-w-2xl flex-col gap-5 p-6">
          <div className="rounded-2xl border border-line bg-surface p-5 shadow-theme-xs">
            <div className="mb-3 flex items-center gap-2 text-xs font-medium text-fg-secondary">
              <LinkCodeIcon className="size-4 text-accent-text" />
              Share code or link
            </div>
            <form
              className="flex gap-2.5"
              onSubmit={(e) => {
                e.preventDefault()
                if (input.trim()) lookup.mutate(input.trim())
              }}
            >
              <Input
                autoFocus
                inputSize="lg"
                value={input}
                onChange={(e) => {
                  setInput(e.target.value)
                  setPreview(null)
                }}
                placeholder="e.g. 7F3K2Q or https://..."
                className="flex-1 font-mono tracking-wider"
              />
              <Button type="submit" variant="primary" size="lg" disabled={!input.trim() || lookup.isPending}>
                <SearchIcon className="size-3.5" />
                Look up
              </Button>
            </form>
          </div>

          {p && (
            <div className="animate-dialog overflow-hidden rounded-2xl border border-line-strong bg-surface shadow-theme-sm">
              <div className="flex flex-col gap-2 border-b border-line bg-sidebar/40 p-5">
                <div className="flex flex-wrap items-center gap-2">
                  <h2 className="text-lg font-semibold tracking-tight text-fg">{p.name}</h2>
                  <Tag tone="accent" dot>
                    Minecraft {p.minecraft}
                  </Tag>
                  <Tag>{loaderText(p.loader)}</Tag>
                </div>
                <div className="text-xs text-muted">
                  Published by <span className="font-medium text-fg-secondary">{p.owner}</span>
                </div>
                {p.description && <p className="mt-1 text-[13px] leading-relaxed text-fg-secondary">{p.description}</p>}
              </div>

              <div className="flex flex-col gap-5 p-5">
                <dl className="grid grid-cols-[120px_1fr] items-baseline gap-y-3 text-[13px]">
                  <dt className="text-xs font-medium text-muted">Latest version</dt>
                  <dd>
                    {p.latest ? (
                      <Tag tone="ok" dot>
                        {p.latest.name}
                      </Tag>
                    ) : (
                      <span className="text-muted">Nothing published yet</span>
                    )}
                  </dd>
                  <dt className="text-xs font-medium text-muted">Servers</dt>
                  <dd className="flex flex-col gap-1.5">
                    {p.servers.length === 0 && <span className="text-muted">None attached</span>}
                    {p.servers.map((srv) => (
                      <div
                        key={srv.name}
                        className="flex flex-wrap items-center gap-2 rounded-lg border border-line bg-bg/60 px-3 py-1.5 text-xs"
                      >
                        <ServerIcon className="size-3.5 text-muted" />
                        <span className={`size-2 rounded-full ${srv.online ? 'bg-ok' : 'bg-subtle'}`} />
                        <span className="font-medium text-fg">{srv.name}</span>
                        <span className="font-mono text-[11px] text-muted">{srv.address}</span>
                        {srv.commit && <span className="ml-auto font-mono text-[11px] text-subtle">{shortId(srv.commit)}</span>}
                      </div>
                    ))}
                  </dd>
                </dl>

                <div className="flex flex-wrap items-center gap-2 border-t border-line pt-4">
                  {mine && (
                    <Button variant="primary" disabled={!p.latest || join.isPending} onClick={() => join.mutate('edit')}>
                      Edit this pack
                    </Button>
                  )}
                  <Button
                    variant={mine ? 'default' : 'primary'}
                    disabled={!p.latest || join.isPending}
                    onClick={() => join.mutate('play')}
                  >
                    <PlayIcon className="size-3.5" />
                    Play this pack
                  </Button>
                  <Button disabled={!p.latest || join.isPending} onClick={() => join.mutate('fork')}>
                    <GitBranchIcon className="size-3.5" />
                    Make my own copy
                  </Button>
                </div>

                <p className="text-xs leading-relaxed text-muted">
                  {mine
                    ? 'Editing continues your pack on this computer from its newest saved version. Playing only follows its updates.'
                    : "Playing keeps you on the owner's updates. Your own copy can be changed freely and can still get updates later."}
                </p>
              </div>
            </div>
          )}
        </div>
      </div>
    </>
  )
}
