import { useQuery } from '@tanstack/react-query'
import { save } from '@tauri-apps/plugin-dialog'
import { useState } from 'react'
import { Button } from '../../components/ui'
import { call } from '../../lib/api'
import { keys, useAction, useApp } from '../../lib/store'
import type { PackInfo } from '../../lib/types'

function Copy({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)
  return (
    <Button
      small
      onClick={async () => {
        await navigator.clipboard.writeText(text)
        setCopied(true)
        setTimeout(() => setCopied(false), 1500)
      }}
    >
      {copied ? 'Copied' : 'Copy'}
    </Button>
  )
}

export function ShareTab({ p }: { p: PackInfo }) {
  const go = useApp((s) => s.go)
  const settings = useQuery({ queryKey: keys.settings, queryFn: () => call('get_settings') })
  const exportMr = useAction((out: string) => call('export_mrpack', { path: p.path, out }), {
    label: 'Exporting .mrpack',
    done: 'Exported',
  })
  const pick = async () => {
    const out = await save({ defaultPath: `${p.name}.mrpack`, filters: [{ name: 'Modrinth pack', extensions: ['mrpack'] }] })
    if (out) exportMr.mutate(out)
  }
  const remote = p.config.remote

  return (
    <div className="flex flex-col">
      <section className="flex flex-col gap-3 border-b border-line px-5 py-5">
        <h3 className="font-semibold">Share code</h3>
        {remote && p.share_link ? (
          <div className="grid max-w-xl grid-cols-[60px_minmax(0,1fr)_auto] items-center gap-x-3 gap-y-2">
            <span className="text-muted">Code</span>
            <span className="font-mono text-2xl tracking-[0.2em] select-text">{remote.code}</span>
            <Copy text={remote.code} />
            <span className="text-muted">Link</span>
            <span className="truncate font-mono text-xs select-text">{p.share_link}</span>
            <Copy text={p.share_link} />
          </div>
        ) : settings.data?.signed_in ? (
          <div className="flex items-center gap-3">
            <span className="text-muted">Publish this pack to get a share code.</span>
            <Button small onClick={() => go({ name: 'pack', path: p.path, tab: 'servers' })}>
              Publish update
            </Button>
          </div>
        ) : (
          <div className="flex items-center gap-3">
            <span className="text-muted">Sign in to a hub to share this pack with a code.</span>
            <Button small onClick={() => go({ name: 'settings' })}>
              Sign in
            </Button>
          </div>
        )}
        {remote && <p className="text-xs text-muted">Friends paste the code in Join with code. They get your updates when you publish.</p>}
      </section>
      <section className="flex flex-col items-start gap-3 px-5 py-5">
        <h3 className="font-semibold">Modrinth pack</h3>
        <p className="text-muted">A .mrpack file works in Prism, Modrinth App and most launchers.</p>
        <Button onClick={pick} disabled={exportMr.isPending}>
          Export .mrpack
        </Button>
      </section>
    </div>
  )
}
