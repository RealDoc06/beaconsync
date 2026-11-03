import { useQuery } from '@tanstack/react-query'
import { open } from '@tauri-apps/plugin-dialog'
import { useDeferredValue, useState } from 'react'
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
      <PageHeader title="Import" />
      <div className="pt-2">
        <Tabs
          value={tab}
          onChange={setTab}
          tabs={[
            { value: 'file', label: 'From a file or folder' },
            { value: 'modrinth', label: 'Modrinth modpacks' },
            ...(hasCf ? [{ value: 'curseforge' as const, label: 'CurseForge modpacks' }] : []),
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
    <div className="grid max-w-3xl grid-cols-2 gap-px border-b border-line bg-line">
      <div className="flex flex-col items-start gap-3 bg-bg p-5">
        <h3 className="font-semibold">Modpack file</h3>
        <p className="text-muted">.mrpack, CurseForge .zip or a Prism Launcher export.</p>
        <Button variant="primary" onClick={pickFile} disabled={file.isPending}>
          Choose file
        </Button>
      </div>
      <div className="flex flex-col items-start gap-3 bg-bg p-5">
        <h3 className="font-semibold">Existing instance</h3>
        <p className="text-muted">A Prism or MultiMC instance folder. Its files stay as they are.</p>
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
    <div className="flex flex-col">
      <div className="p-5 pb-3">
        <Input placeholder="Search Modrinth modpacks" value={q} onChange={(e) => setQ(e.target.value)} className="w-full max-w-md" />
      </div>
      <ErrorLine error={results.error} />
      {results.data?.hits.map((h) => (
        <ResultRow
          key={h.project_id}
          icon={h.icon_url}
          title={h.title}
          meta={`${h.author} · ${count(h.downloads)} downloads`}
          description={h.description}
          action={
            <Button small onClick={() => install.mutate(h.project_id)} disabled={install.isPending}>
              Install
            </Button>
          }
        />
      ))}
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
    <div className="flex flex-col">
      <div className="p-5 pb-3">
        <Input placeholder="Search CurseForge modpacks" value={q} onChange={(e) => setQ(e.target.value)} className="w-full max-w-md" />
      </div>
      <ErrorLine error={results.error} />
      {results.data?.length === 0 && <Empty>No modpacks found</Empty>}
      {results.data?.map((m) => (
        <ResultRow
          key={m.id}
          icon={m.logo?.thumbnailUrl}
          title={m.name}
          meta={`${m.authors[0]?.name ?? ''} · ${count(m.downloadCount)} downloads`}
          description={m.summary}
          action={
            <Button small onClick={() => install.mutate(m.id)} disabled={install.isPending}>
              Install
            </Button>
          }
        />
      ))}
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
    <div className="flex items-center gap-3 border-t border-line px-5 py-2.5">
      <Thumb src={icon} size={36} />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className="truncate font-medium">{title}</span>
          <span className="shrink-0 text-xs text-muted">{meta}</span>
        </div>
        <div className="truncate text-xs text-muted">{description}</div>
      </div>
      {action}
    </div>
  )
}
