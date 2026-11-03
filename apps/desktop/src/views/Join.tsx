import { useState } from 'react'
import { PageHeader } from '../components/PageHeader'
import { Button, Input } from '../components/ui'
import { call } from '../lib/api'
import { loaderText, shortId } from '../lib/format'
import { keys, useAction, useApp } from '../lib/store'
import type { JoinPreview } from '../lib/types'

export function JoinView({ initial }: { initial?: string }) {
  const go = useApp((s) => s.go)
  const [input, setInput] = useState(initial ?? '')
  const [preview, setPreview] = useState<JoinPreview | null>(null)
  const lookup = useAction((code: string) => call('preview_code', { input: code }), {
    label: 'Looking up code',
    onSuccess: setPreview,
  })
  const join = useAction((fork: boolean) => call('join_pack', { input, fork }), {
    label: 'Downloading pack',
    invalidate: [keys.library],
    done: 'Pack ready',
    onSuccess: (path) => go({ name: 'pack', path, tab: 'mods' }),
  })
  const p = preview?.pack

  return (
    <>
      <PageHeader title="Join with code" />
      <div className="flex max-w-xl flex-col gap-5 p-5">
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault()
            if (input.trim()) lookup.mutate(input.trim())
          }}
        >
          <Input
            autoFocus
            value={input}
            onChange={(e) => {
              setInput(e.target.value)
              setPreview(null)
            }}
            placeholder="Code or link, e.g. 7F3K2Q"
            className="flex-1 font-mono"
          />
          <Button type="submit" disabled={!input.trim() || lookup.isPending}>
            Look up
          </Button>
        </form>

        {p && (
          <div className="flex flex-col gap-4 border-t border-line pt-5">
            <div>
              <div className="text-lg font-semibold">{p.name}</div>
              <div className="text-muted">
                by {p.owner} · Minecraft {p.minecraft} · {loaderText(p.loader)}
              </div>
              {p.description && <p className="mt-2">{p.description}</p>}
            </div>
            <dl className="grid grid-cols-[120px_1fr] gap-y-1.5">
              <dt className="text-muted">Latest version</dt>
              <dd>{p.latest ? p.latest.name : 'Nothing published yet'}</dd>
              <dt className="text-muted">Servers</dt>
              <dd className="flex flex-col">
                {p.servers.length === 0 && <span className="text-muted">None</span>}
                {p.servers.map((s) => (
                  <span key={s.name}>
                    <span className={s.online ? 'text-ok' : 'text-muted'}>●</span> {s.name}{' '}
                    <span className="font-mono text-xs text-muted">{s.address}</span>
                    {s.commit && <span className="text-xs text-muted"> · {shortId(s.commit)}</span>}
                  </span>
                ))}
              </dd>
            </dl>
            <div className="flex gap-2">
              <Button variant="primary" disabled={!p.latest || join.isPending} onClick={() => join.mutate(false)}>
                Play this pack
              </Button>
              <Button disabled={!p.latest || join.isPending} onClick={() => join.mutate(true)}>
                Make my own copy
              </Button>
            </div>
            <p className="text-xs text-muted">
              Playing keeps you on the owner's updates. Your own copy can be changed freely and can still get updates later.
            </p>
          </div>
        )}
      </div>
    </>
  )
}
