import { useQuery } from '@tanstack/react-query'
import { save } from '@tauri-apps/plugin-dialog'
import { useEffect, useRef, useState } from 'react'
import { CheckIcon, CopyIcon, DownloadIcon, LinkCodeIcon, ShareIcon } from '../../components/Icons'
import { Button } from '../../components/ui'
import { call } from '../../lib/api'
import { keys, useAction, useApp } from '../../lib/store'
import type { PackInfo } from '../../lib/types'
import { useShare } from './Servers'

function Copy({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const setNotice = useApp((s) => s.set)
  useEffect(() => () => clearTimeout(timer.current), [])
  return (
    <Button
      small
      onClick={async () => {
        try {
          await navigator.clipboard.writeText(text)
          setCopied(true)
          clearTimeout(timer.current)
          timer.current = setTimeout(() => setCopied(false), 1500)
        } catch {
          setNotice({ notice: { kind: 'error', text: 'Could not copy to clipboard' } })
        }
      }}
    >
      {copied ? <CheckIcon className="size-3 text-ok" /> : <CopyIcon className="size-3" />}
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
  const share = useShare(p.path)

  return (
    <div className="min-h-0 flex-1 overflow-auto">
      <div className="mx-auto flex max-w-2xl flex-col gap-5 p-6">
        <section className="overflow-hidden rounded-2xl border border-line bg-surface shadow-theme-xs">
          <div className="flex items-center gap-3 border-b border-line bg-sidebar/50 px-5 py-3.5">
            <div className="grid size-7 place-items-center rounded-lg border border-accent-border bg-accent-soft text-accent-text">
              <ShareIcon className="size-4" />
            </div>
            <div>
              <h3 className="text-[13.5px] font-semibold text-fg">Share code</h3>
              <p className="text-xs text-muted">Invite friends to play or fork this pack via your hub.</p>
            </div>
          </div>

          <div className="flex flex-col gap-4 p-5">
            {remote && p.share_link ? (
              <>
                <div className="flex items-center justify-between gap-4 rounded-xl border border-accent-border bg-accent-soft/60 px-4 py-3.5">
                  <div className="flex flex-col gap-0.5">
                    <span className="text-[10.5px] font-semibold uppercase tracking-[0.08em] text-accent-text">Code</span>
                    <span className="font-mono text-2xl font-bold tracking-[0.22em] text-fg select-text">{remote.code}</span>
                  </div>
                  <Copy text={remote.code} />
                </div>

                <div className="flex items-center gap-3 rounded-xl border border-line bg-bg/60 px-3.5 py-2.5">
                  <LinkCodeIcon className="size-4 shrink-0 text-muted" />
                  <span className="min-w-0 flex-1 truncate font-mono text-xs text-fg-secondary select-text">
                    {p.share_link}
                  </span>
                  <Copy text={p.share_link} />
                </div>

                <p className="text-xs leading-relaxed text-muted">
                  Friends paste the code in <span className="font-medium text-fg-secondary">Join with code</span>. They
                  automatically receive your updates whenever you publish a new version.
                </p>
              </>
            ) : settings.data?.signed_in ? (
              <div className="flex items-center justify-between gap-3 rounded-xl border border-line bg-bg/60 px-4 py-3">
                <span className="text-xs text-muted">This pack is local only and not on the hub yet.</span>
                <Button small variant="primary" disabled={share.isPending} onClick={() => share.mutate(undefined)}>
                  Share on hub
                </Button>
              </div>
            ) : (
              <div className="flex items-center justify-between gap-3 rounded-xl border border-line bg-bg/60 px-4 py-3">
                <span className="text-xs text-muted">Sign in to a hub in Settings to generate a share code.</span>
                <Button small variant="primary" onClick={() => go({ name: 'settings' })}>
                  Sign in
                </Button>
              </div>
            )}
          </div>
        </section>

        <section className="overflow-hidden rounded-2xl border border-line bg-surface shadow-theme-xs">
          <div className="flex items-center gap-3 border-b border-line bg-sidebar/50 px-5 py-3.5">
            <div className="grid size-7 place-items-center rounded-lg border border-line-strong bg-surface-2 text-fg-secondary">
              <DownloadIcon className="size-4" />
            </div>
            <div>
              <h3 className="text-[13.5px] font-semibold text-fg">Export Modrinth pack</h3>
              <p className="text-xs text-muted">Save a portable .mrpack archive for Prism, Modrinth App, and other launchers.</p>
            </div>
          </div>
          <div className="p-5">
            <Button onClick={pick} disabled={exportMr.isPending}>
              <DownloadIcon className="size-3.5" />
              Export .mrpack
            </Button>
          </div>
        </section>
      </div>
    </div>
  )
}
