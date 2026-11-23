import { useQuery } from '@tanstack/react-query'
import { open } from '@tauri-apps/plugin-dialog'
import { useDeferredValue, useState } from 'react'
import { DownloadIcon, FileArchiveIcon, FolderIcon, SearchIcon } from '../components/Icons'
import { PageHeader } from '../components/PageHeader'
import { Button, Empty, ErrorLine, Input, Tabs, Thumb } from '../components/ui'
import { call } from '../lib/api'
import { count } from '../lib/format'
import { keys, useAction, useApp } from '../lib/store'

type Source = 'file' | 'modrinth' | 'curseforge'

/** Shared "open the new pack when done" import action. */
function useImport<V>(fn: (v: V) => Promise<string>, label: string) {
  const go = useApp((s) => s.go)
  return useAction(fn, {
    label,
    invalidate: [keys.library],
    done: 'Imported',
    onSuccess: (path) => go({ name: 'pack', path, tab: 'mods' }),
  })
}

export function ImportView() {
  const [tab, setTab] = useState<Source>('file')
  const settings = useQuery({ queryKey: keys.settings, queryFn: () => call('get_settings') })
  const hasCf = (settings.data?.curseforge_key ?? '') !== ''
  return (
    <>
      <PageHeader title="Import" subtitle="Bring in an existing pack from disk, Modrinth, or CurseForge" />
      <div className="pt-1">
        <Tabs
          value={tab}
          onChange={setTab}
          tabs={[
            { value: 'file', label: 'From file or folder', icon: <FolderIcon className="size-3.5" /> },
            { value: 'modrinth', label: 'Modrinth modpacks', icon: <DownloadIcon className="size-3.5" /> },
            ...(hasCf
              ? [{ value: 'curseforge' as const, label: 'CurseForge modpacks', icon: <DownloadIcon className="size-3.5" /> }]
              : []),
          ]}
        />
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {tab === 'file' && <FromDisk />}
        {tab === 'modrinth' && <Modrinth />}
        {tab === 'curseforge' && <CurseForge />}
      </div>
    </>
  )
}

function FromDisk() {
  const file = useImport((path: string) => call('import_file', { file: path }), 'Importing')
  const folder = useImport((dir: string) => call('adopt_instance', { dir }), 'Tracking folder')
  const pickFile = async () => {
    const path = await open({ filters: [{ name: 'Modpack', extensions: ['mrpack', 'zip'] }] })
    if (path) file.mutate(path)
  }
  const pickFolder = async () => {
    const dir = await open({ directory: true })
    if (dir) folder.mutate(dir)
  }
  return (
    <div className="grid max-w-3xl grid-cols-2 gap-4 p-6">
      <div className="flex flex-col items-start justify-between gap-5 rounded-2xl border border-line bg-surface p-5 shadow-theme-xs">
        <div className="flex flex-col gap-2.5">
          <div className="grid size-10 place-items-center rounded-xl border border-accent-border bg-accent-soft text-accent-text">
            <FileArchiveIcon className="size-5" />
          </div>
          <h3 className="text-sm font-semibold text-fg">Modpack file</h3>
          <p className="text-xs leading-relaxed text-muted">
            Import a <span className="font-mono text-fg-secondary">.mrpack</span>, CurseForge{' '}
            <span className="font-mono text-fg-secondary">.zip</span>, or a Prism Launcher export.
          </p>
        </div>
        <Button variant="primary" onClick={pickFile} disabled={file.isPending}>
          Choose file
        </Button>
      </div>
      <div className="flex flex-col items-start justify-between gap-5 rounded-2xl border border-line bg-surface p-5 shadow-theme-xs">
        <div className="flex flex-col gap-2.5">
          <div className="grid size-10 place-items-center rounded-xl border border-line-strong bg-surface-2 text-fg-secondary">
            <FolderIcon className="size-5" />
          </div>
          <h3 className="text-sm font-semibold text-fg">Existing instance</h3>
          <p className="text-xs leading-relaxed text-muted">
            Track an existing Prism or MultiMC instance folder. Its files stay untouched right where they are.
          </p>
        </div>
        <Button onClick={pickFolder} disabled={folder.isPending}>
          Choose folder
        </Button>
      </div>
    </div>
  )
}

function Modrinth() {
  const [q, setQ] = useState('')
  const query = useDeferredValue(q.trim())
  const results = useQuery({
    queryKey: ['mr-packs', query],
    queryFn: () => call('search_modrinth', { query, projectType: 'modpack', minecraft: null, loader: null, offset: 0 }),
    staleTime: 300_000,
  })
  const install = useImport((project: string) => call('install_modrinth_pack', { project }), 'Installing modpack')
  return (
    <div className="flex flex-col gap-4 p-6">
      <Input
        icon={<SearchIcon className="size-3.5" />}
        placeholder="Search Modrinth modpacks"
        value={q}
        onChange={(e) => setQ(e.target.value)}
        className="w-full max-w-md"
      />
      <ErrorLine error={results.error} />
      {results.data?.hits.length === 0 && (
        <Empty icon={<SearchIcon className="size-5" />} title="No modpacks found">
          Try a different keyword.
        </Empty>
      )}
      {results.data && results.data.hits.length > 0 && (
        <div className="overflow-hidden rounded-2xl border border-line bg-surface shadow-theme-xs divide-y divide-line">
          {results.data.hits.map((h) => (
            <ResultRow
              key={h.project_id}
              icon={h.icon_url}
              title={h.title}
              meta={`${h.author} · ${count(h.downloads)} downloads`}
              description={h.description}
              action={
                <Button small variant="primary" onClick={() => install.mutate(h.project_id)} disabled={install.isPending}>
                  <DownloadIcon className="size-3.5" />
                  Install
                </Button>
              }
            />
          ))}
        </div>
      )}
    </div>
  )
}

function CurseForge() {
  const [q, setQ] = useState('')
  const query = useDeferredValue(q.trim())
  const results = useQuery({
    queryKey: ['cf-packs', query],
    queryFn: () => call('search_curseforge_packs', { query }),
    staleTime: 300_000,
  })
  const install = useImport((id: number) => call('install_curseforge_pack', { id }), 'Installing modpack')
  return (
    <div className="flex flex-col gap-4 p-6">
      <Input
        icon={<SearchIcon className="size-3.5" />}
        placeholder="Search CurseForge modpacks"
        value={q}
        onChange={(e) => setQ(e.target.value)}
        className="w-full max-w-md"
      />
      <ErrorLine error={results.error} />
      {results.data?.length === 0 && (
        <Empty icon={<SearchIcon className="size-5" />} title="No modpacks found">
          Try a different keyword.
        </Empty>
      )}
      {results.data && results.data.length > 0 && (
        <div className="overflow-hidden rounded-2xl border border-line bg-surface shadow-theme-xs divide-y divide-line">
          {results.data.map((m) => (
            <ResultRow
              key={m.id}
              icon={m.logo?.thumbnailUrl}
              title={m.name}
              meta={`${m.authors[0]?.name ?? ''} · ${count(m.downloadCount)} downloads`}
              description={m.summary}
              action={
                <Button small variant="primary" onClick={() => install.mutate(m.id)} disabled={install.isPending}>
                  <DownloadIcon className="size-3.5" />
                  Install
                </Button>
              }
            />
          ))}
        </div>
      )}
    </div>
  )
}

export function ResultRow({
  icon,
  title,
  meta,
  description,
  action,
}: {
  icon: string | null | undefined
  title: string
  meta: string
  description: string
  action: React.ReactNode
}) {
  return (
    <div className="flex items-center gap-3.5 px-4 py-3 transition-colors hover:bg-hover">
      <Thumb src={icon} size={40} />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className="truncate font-semibold text-fg">{title}</span>
          <span className="shrink-0 text-xs text-muted">{meta}</span>
        </div>
        <div className="mt-0.5 truncate text-xs text-muted">{description}</div>
      </div>
      {action}
    </div>
  )
}
