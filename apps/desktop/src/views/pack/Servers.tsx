import { useQuery } from '@tanstack/react-query'
import { save } from '@tauri-apps/plugin-dialog'
import { useState } from 'react'
import { Button, Dialog, Empty, ErrorLine, Field, Input, Select, Toggle } from '../../components/ui'
import { call } from '../../lib/api'
import { ago, shortId } from '../../lib/format'
import { keys, packKeys, useAction } from '../../lib/store'
import type { PackInfo, Release, Server } from '../../lib/types'

export function ServersTab({ p }: { p: PackInfo }) {
  const path = p.path
  const published = !!p.config.remote
  const [publishing, setPublishing] = useState(false)
  const [deploying, setDeploying] = useState(false)

  const servers = useQuery({
    queryKey: [...keys.pack(path), 'servers'],
    queryFn: () => call('servers', { path }),
    enabled: published,
    refetchInterval: 15_000,
  })
  const releases = useQuery({ queryKey: [...keys.pack(path), 'releases'], queryFn: () => call('releases', { path }), enabled: published })
  const exportPack = useAction((out: string) => call('export_server_pack', { path, out }), {
    label: 'Building server pack',
    done: 'Server pack exported',
  })
  const pickExport = async () => {
    const out = await save({ defaultPath: `${p.name}-server.zip`, filters: [{ name: 'Zip', extensions: ['zip'] }] })
    if (out) exportPack.mutate(out)
  }
  const releaseName = (commit: string | null) => (commit ? (releases.data?.find((r) => r.commit === commit)?.name ?? shortId(commit)) : 'Nothing yet')

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-auto">
      <div className="flex items-center gap-2 px-5 py-3">
        <Button variant="primary" onClick={() => setPublishing(true)}>
          Publish update
        </Button>
        <Button onClick={() => setDeploying(true)} disabled={!published || !releases.data?.length || !servers.data?.length}>
          Deploy to servers
        </Button>
        <span className="flex-1" />
        <Button onClick={pickExport} disabled={exportPack.isPending}>
          Export server pack
        </Button>
      </div>

      <div className="border-t border-line">
        {!published && <Empty>Publish this pack to share it and deploy it to servers.</Empty>}
        <ErrorLine error={servers.error} />
        {published && servers.data?.length === 0 && <Empty>No servers yet. Install one with the steps below.</Empty>}
        {servers.data?.map((s) => (
          <ServerRow key={s.id} s={s} version={releaseName(s.current_commit)} target={releaseName(s.target_commit)} />
        ))}
      </div>

      <section className="mt-auto flex flex-col gap-2 border-t border-line px-5 py-4">
        <h3 className="font-semibold">Install on a server</h3>
        <p className="text-muted">On the server machine, install the beacon CLI, then run:</p>
        <pre className="rounded-md border border-line bg-raise px-3 py-2 font-mono text-xs leading-6 select-text">
          beacon login --hub {p.config.remote?.hub ?? '<hub url>'}{'\n'}beacon server install --pack {p.config.remote?.code ?? '<share code>'}
        </pre>
        <p className="text-xs text-muted">
          beacon login shows a code. Approve it in Settings. The server then appears here and follows your deploys.
        </p>
      </section>

      {publishing && <PublishDialog path={path} onClose={() => setPublishing(false)} />}
      {deploying && releases.data && servers.data && (
        <DeployDialog path={path} releases={releases.data} servers={servers.data} onClose={() => setDeploying(false)} />
      )}
    </div>
  )
}

function ServerRow({ s, version, target }: { s: Server; version: string; target: string }) {
  const pending = s.target_commit && s.target_commit !== s.current_commit
  const tone = s.status === 'failed' ? 'text-bad' : s.status === 'applying' || pending ? 'text-warn' : 'text-muted'
  return (
    <div className="grid h-12 grid-cols-[minmax(0,1fr)_minmax(0,180px)_160px_minmax(0,1fr)] items-center gap-3 border-b border-line px-5">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <span className={s.online ? 'text-ok' : 'text-[#444]'}>●</span>
          <span className="truncate font-medium">{s.name}</span>
        </div>
        <div className="truncate pl-4 font-mono text-[11px] text-muted">{s.address}</div>
      </div>
      <span className="truncate">
        {version}
        {pending && <span className="text-warn"> → {target}</span>}
      </span>
      <span className={`text-xs ${tone}`}>
        {s.online ? s.status : 'offline'}
        {s.last_seen && !s.online && ` · ${ago(s.last_seen)}`}
      </span>
      <span className="truncate text-xs text-muted" title={s.message}>
        {s.message}
      </span>
    </div>
  )
}

function PublishDialog({ path, onClose }: { path: string; onClose: () => void }) {
  const [name, setName] = useState('')
  const [notes, setNotes] = useState('')
  const publish = useAction(() => call('publish', { path, name, notes }), {
    label: 'Publishing',
    invalidate: packKeys(path),
    done: (r) => `Published ${r.name}`,
    onSuccess: onClose,
  })
  return (
    <Dialog
      title="Publish update"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!name.trim() || publish.isPending} onClick={() => publish.mutate(undefined)}>
            Publish
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <Field label="Version name">
          <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="1.2.0" />
        </Field>
        <Field label="Notes">
          <textarea
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            rows={4}
            placeholder="What players get in this update"
            className="resize-none rounded-md border border-line-strong bg-bg p-2.5 outline-none placeholder:text-[#555] focus:border-[#666]"
          />
        </Field>
        <p className="text-xs text-muted">Publishes the last saved version. Players get it next time they open the pack.</p>
      </div>
    </Dialog>
  )
}

function DeployDialog({ path, releases, servers, onClose }: { path: string; releases: Release[]; servers: Server[]; onClose: () => void }) {
  const newest = [...releases].sort((a, b) => b.created - a.created)
  const [release, setRelease] = useState(newest[0]?.id ?? '')
  const [all, setAll] = useState(true)
  const [chosen, setChosen] = useState<ReadonlySet<string>>(new Set())
  const deploy = useAction(() => call('deploy', { path, release, servers: all ? null : [...chosen] }), {
    label: 'Deploying',
    invalidate: [[...keys.pack(path), 'servers']],
    done: (r) => `Deploying to ${r.servers.length} server${r.servers.length === 1 ? '' : 's'}`,
    onSuccess: onClose,
  })
  const flip = (id: string) =>
    setChosen((s) => {
      const next = new Set(s)
      if (!next.delete(id)) next.add(id)
      return next
    })
  return (
    <Dialog
      title="Deploy to servers"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!release || (!all && chosen.size === 0) || deploy.isPending} onClick={() => deploy.mutate(undefined)}>
            Deploy
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <Field label="Version">
          <Select value={release} onChange={setRelease} options={newest.map((r) => ({ value: r.id, label: `${r.name} · ${ago(r.created)}` }))} />
        </Field>
        <Toggle checked={all} onChange={setAll} label={`All servers (${servers.length})`} />
        {!all && (
          <div className="flex flex-col border-t border-line">
            {servers.map((s) => (
              <label key={s.id} className="flex h-9 items-center gap-2 border-b border-line">
                <input type="checkbox" checked={chosen.has(s.id)} onChange={() => flip(s.id)} className="accent-[#7ee8ff]" />
                <span>{s.name}</span>
                <span className="ml-auto text-xs text-muted">{s.online ? 'online' : 'offline'}</span>
              </label>
            ))}
          </div>
        )}
        <p className="text-xs text-muted">Each server warns players, restarts with the new version and keeps one backup.</p>
      </div>
    </Dialog>
  )
}
