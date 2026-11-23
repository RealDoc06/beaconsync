import { useQuery } from '@tanstack/react-query'
import { save } from '@tauri-apps/plugin-dialog'
import { useEffect, useRef, useState } from 'react'
import {
  CheckIcon,
  ChevronRightIcon,
  CopyIcon,
  DownloadIcon,
  RefreshIcon,
  RocketIcon,
  ServerIcon,
  TerminalIcon,
  UploadIcon,
} from '../../components/Icons'
import { Button, Dialog, Empty, ErrorLine, Field, Input, Select, Tag, Textarea, Toggle, type TagTone } from '../../components/ui'
import { useUpdate } from '../../components/UpdateButton'
import { call } from '../../lib/api'
import { ago, nextVersion, shortId } from '../../lib/format'
import { askConfirm, keys, packKeys, useAction, useApp } from '../../lib/store'
import type { PackInfo, Release, Server } from '../../lib/types'
import { ServerPanel } from './ServerPanel'

/** Shares a pack tracked before sharing was automatic (or whose automatic share failed). */
export function useShare(path: string) {
  return useAction(() => call('share_pack', { path }), { label: 'Sharing', invalidate: packKeys(path), done: 'Shared on the hub' })
}

export function ServersTab({ p }: { p: PackInfo }) {
  const path = p.path
  const published = !!p.config.remote
  const [publishing, setPublishing] = useState(false)
  const [deploying, setDeploying] = useState(false)
  const [open, setOpen] = useState<string | null>(null)
  const [copiedCmd, setCopiedCmd] = useState(false)
  const copyTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const setNotice = useApp((s) => s.set)
  useEffect(() => () => clearTimeout(copyTimer.current), [])
  const share = useShare(path)

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
  const latest = useUpdate()?.latest ?? null
  const outdated = servers.data?.filter((s) => s.online && behind(s, latest)) ?? []
  const updateAll = useAction(
    () => Promise.all(outdated.map((s) => call('server_action', { path, server: s.id, action: { kind: 'update', arg: latest ?? '' } }))),
    {
      label: 'Updating beacon',
      invalidate: [[...keys.pack(path), 'servers']],
      done: () => `Updating beacon on ${outdated.length} server${outdated.length === 1 ? '' : 's'}`,
    },
  )

  const installSnippet = [
    'curl -fsSL https://github.com/RealDoc06/beaconsync/releases/latest/download/install.sh | bash',
    'export PATH="$HOME/.local/bin:$PATH"',
    `beacon login --hub ${p.config.remote?.hub ?? '<hub url>'}`,
    `beacon server install --pack ${p.config.remote?.code ?? '<share code>'}`,
  ].join('\n')

  const opened = servers.data?.find((s) => s.id === open)
  if (opened)
    return (
      <ServerPanel
        path={path}
        server={opened}
        version={releaseName(opened.current_commit)}
        latest={behind(opened, latest) ? latest : null}
        onBack={() => setOpen(null)}
      />
    )

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-auto">
      <div className="flex flex-wrap items-center gap-2 border-b border-line px-6 py-3">
        <Button variant="primary" small disabled={releases.isLoading} onClick={() => setPublishing(true)}>
          <UploadIcon className="size-3.5" />
          Publish update
        </Button>
        <Button small onClick={() => setDeploying(true)} disabled={!published || !releases.data?.length || !servers.data?.length}>
          <RocketIcon className="size-3.5" />
          Deploy to servers
        </Button>
        <span className="flex-1" />
        {latest && outdated.length > 0 && (
          <Button
            small
            disabled={updateAll.isPending}
            onClick={() =>
              askConfirm({
                title: `Update beacon to ${latest}?`,
                body: `${outdated.map((s) => s.name).join(', ')} download beacon ${latest} and check it, then warn players and restart Minecraft on it. Takes about a minute.`,
                action: 'Update',
                onConfirm: () => updateAll.mutate(undefined),
              })
            }
          >
            <RefreshIcon className="size-3.5" />
            Update beacon ({outdated.length})
          </Button>
        )}
        <Button small onClick={pickExport} disabled={exportPack.isPending}>
          <DownloadIcon className="size-3.5" />
          Export server zip
        </Button>
      </div>

      <div className="flex flex-1 flex-col gap-5 p-6">
        {!published && (
          <div className="rounded-2xl border border-line bg-surface shadow-theme-xs">
            <Empty
              icon={<ServerIcon className="size-5" />}
              title="Not on the hub yet"
              action={
                <Button variant="primary" disabled={share.isPending} onClick={() => share.mutate(undefined)}>
                  Share on hub
                </Button>
              }
            >
              Share this pack on your hub to publish releases and install auto-updating servers.
            </Empty>
          </div>
        )}

        <ErrorLine error={servers.error} />

        {published && servers.data?.length === 0 && (
          <div className="rounded-2xl border border-line bg-surface shadow-theme-xs">
            <Empty icon={<ServerIcon className="size-5" />} title="No servers attached">
              {releases.data?.length
                ? 'Install a server using the commands below to see live status, console, and backups here.'
                : 'Publish your first release, then install a server with the commands below.'}
            </Empty>
          </div>
        )}

        {servers.data && servers.data.length > 0 && (
          <div className="@container overflow-hidden rounded-2xl border border-line bg-surface shadow-theme-xs divide-y divide-line">
            {servers.data.map((s) => (
              <ServerRow
                key={s.id}
                s={s}
                version={releaseName(s.current_commit)}
                target={releaseName(s.target_commit)}
                latest={behind(s, latest) ? latest : null}
                onOpen={() => setOpen(s.id)}
              />
            ))}
          </div>
        )}

        <section className="mt-auto overflow-hidden rounded-2xl border border-line bg-surface shadow-theme-xs">
          <div className="flex items-center justify-between border-b border-line bg-sidebar/50 px-4 py-2.5">
            <div className="flex items-center gap-2 text-xs font-semibold text-fg">
              <TerminalIcon className="size-3.5 text-accent-text" />
              Install on a server (Linux or macOS)
            </div>
            <Button
              small
              variant="ghost"
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(installSnippet)
                  setCopiedCmd(true)
                  clearTimeout(copyTimer.current)
                  copyTimer.current = setTimeout(() => setCopiedCmd(false), 1500)
                } catch {
                  setNotice({ notice: { kind: 'error', text: 'Could not copy to clipboard' } })
                }
              }}
            >
              {copiedCmd ? <CheckIcon className="size-3 text-ok" /> : <CopyIcon className="size-3" />}
              {copiedCmd ? 'Copied' : 'Copy'}
            </Button>
          </div>
          <div className="flex flex-col gap-2.5 p-4">
            <pre className="overflow-x-auto rounded-xl border border-line bg-console px-3.5 py-2.5 font-mono text-xs leading-6 text-fg-secondary select-text">
              {installSnippet}
            </pre>
            <p className="text-xs text-muted">
              <span className="font-mono text-fg-secondary">beacon login</span> shows a code to approve in Settings. The
              server then appears above and follows your deploys automatically.
            </p>
          </div>
        </section>
      </div>

      {publishing && <PublishDialog path={path} released={releases.data?.map((r) => r.name) ?? []} onClose={() => setPublishing(false)} />}
      {deploying && releases.data && servers.data && (
        <DeployDialog path={path} releases={releases.data} servers={servers.data} onClose={() => setDeploying(false)} />
      )}
    </div>
  )
}

/** True when the server's beacon isn't the newest release. Agents too old to report a version can't update themselves. */
function behind(s: Server, latest: string | null) {
  return !!latest && !!s.agent_version && s.agent_version !== latest
}

function ServerRow({
  s,
  version,
  target,
  latest,
  onOpen,
}: {
  s: Server
  version: string
  target: string
  /** Newer beacon for this server, if any. */
  latest: string | null
  onOpen: () => void
}) {
  const pending = s.target_commit && s.target_commit !== s.current_commit
  const tone: TagTone = !s.online
    ? 'muted'
    : s.status === 'failed'
      ? 'bad'
      : s.status === 'applying' || s.status === 'stopped' || pending
        ? 'warn'
        : 'ok'

  return (
    <button
      type="button"
      onClick={onOpen}
      title="Open console and backups"
      className="group grid w-full grid-cols-[minmax(140px,1.2fr)_minmax(120px,1fr)_auto_minmax(0,1fr)_auto] @max-[760px]:grid-cols-[minmax(140px,1.2fr)_minmax(110px,1fr)_auto_auto] items-center gap-3 px-4 py-3.5 text-left transition-colors hover:bg-hover"
    >
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <span className={`size-2 shrink-0 rounded-full ${s.online ? 'bg-ok' : 'bg-subtle'}`} />
          <span className="truncate font-semibold text-fg group-hover:text-accent-text">{s.name}</span>
        </div>
        <div className="mt-0.5 truncate pl-4 font-mono text-[11px] text-muted">{s.address}</div>
      </div>

      <div className="min-w-0">
        <div className="truncate text-[13px] font-medium text-fg">
          {version}
          {pending && <span className="text-warn"> → {target}</span>}
        </div>
        <div className="mt-0.5 truncate font-mono text-[11px] text-muted">
          beacon {s.agent_version ?? 'unknown'}
          {latest && <span className="text-warn"> → {latest}</span>}
        </div>
      </div>

      <div>
        <Tag tone={tone} dot>
          {s.online ? s.status : 'offline'}
          {s.last_seen && !s.online && ` · ${ago(s.last_seen)}`}
        </Tag>
      </div>

      <span className="truncate text-xs text-muted @max-[760px]:hidden" title={s.message}>
        {s.message}
      </span>

      <span className="inline-flex items-center gap-1 text-xs font-medium text-muted transition-colors group-hover:text-fg">
        Manage
        <ChevronRightIcon className="size-3.5" />
      </span>
    </button>
  )
}

/** `released` are the release names, newest first; the version field starts at the next one. */
function PublishDialog({ path, released, onClose }: { path: string; released: string[]; onClose: () => void }) {
  const [name, setName] = useState(() => nextVersion(released))
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
      subtitle="Publishes your latest saved snapshot to the hub."
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
          {(id) => <Input id={id} value={name} onChange={(e) => setName(e.target.value)} placeholder="v1.0" className="font-mono" />}
        </Field>
        <Field label="Release notes">
          {(id) => (
            <Textarea
              id={id}
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              rows={4}
              placeholder="What players get in this update"
              className="resize-none"
            />
          )}
        </Field>
        <p className="text-xs text-muted">Players see this release next time they open the pack or in game.</p>
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
      subtitle="Each server warns players, backs up, and restarts cleanly."
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
        <Field label="Release version">
          {(id) => (
            <Select
              id={id}
              value={release}
              onChange={setRelease}
              options={newest.map((r) => ({ value: r.id, label: `${r.name} · ${ago(r.created)}` }))}
            />
          )}
        </Field>
        <Toggle checked={all} onChange={setAll} label={`All servers (${servers.length})`} />
        {!all && (
          <div className="overflow-hidden rounded-xl border border-line bg-bg/60 divide-y divide-line">
            {servers.map((s) => (
              <label key={s.id} className="flex h-10 items-center gap-2.5 px-3 transition-colors hover:bg-hover">
                <input type="checkbox" checked={chosen.has(s.id)} onChange={() => flip(s.id)} className="accent-accent" />
                <span className="font-medium text-fg">{s.name}</span>
                <Tag tone={s.online ? 'ok' : 'muted'} dot className="ml-auto">
                  {s.online ? 'online' : 'offline'}
                </Tag>
              </label>
            ))}
          </div>
        )}
        <p className="text-xs text-muted">If an update fails to start, the server rolls back to its backup automatically.</p>
      </div>
    </Dialog>
  )
}
