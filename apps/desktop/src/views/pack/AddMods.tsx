import { useQuery } from '@tanstack/react-query'
import { useDeferredValue, useMemo, useState } from 'react'
import { Button, ErrorLine, Input, Segmented, Thumb } from '../../components/ui'
import { call } from '../../lib/api'
import { count } from '../../lib/format'
import { packKeys, useAction } from '../../lib/store'
import type { FileRow, PackInfo, ProjectType } from '../../lib/types'

const TYPES = [
  { value: 'mod', label: 'Mods' },
  { value: 'resourcepack', label: 'Resource packs' },
  { value: 'shader', label: 'Shaders' },
] as const satisfies readonly { value: ProjectType; label: string }[]

/** Side panel searching Modrinth for content matching the pack's Minecraft version and loader. */
export function AddMods({ p, installed }: { p: PackInfo; installed: FileRow[] }) {
  const [type, setType] = useState<ProjectType>('mod')
  const [q, setQ] = useState('')
  const query = useDeferredValue(q.trim())
  const [added, setAdded] = useState<ReadonlySet<string>>(new Set())
  const have = useMemo(
    () => new Set(installed.flatMap((f) => (f.source?.type === 'modrinth' ? [f.source.project] : []))),
    [installed],
  )

  const results = useQuery({
    queryKey: ['mr-search', type, query, p.minecraft, p.loader.kind],
    queryFn: () =>
      call('search_modrinth', {
        query,
        projectType: type,
        minecraft: p.minecraft,
        loader: type === 'mod' ? p.loader.kind : null,
        offset: 0,
      }),
    staleTime: 300_000,
  })
  const add = useAction((hit: { id: string; title: string }) => call('add_project', { path: p.path, project: hit.id, projectType: type }), {
    label: 'Downloading',
    invalidate: packKeys(p.path),
    done: (names) => `Added ${names.join(', ')}`,
    onSuccess: (_, hit) => setAdded((s) => new Set(s).add(hit.id)),
  })

  return (
    <aside className="flex w-[380px] shrink-0 flex-col border-l border-line">
      <div className="flex flex-col gap-2 p-3">
        <Segmented value={type} onChange={setType} options={TYPES} />
        <Input autoFocus placeholder={`Search Modrinth for ${p.minecraft}`} value={q} onChange={(e) => setQ(e.target.value)} className="w-full" />
      </div>
      <div className="min-h-0 flex-1 overflow-auto border-t border-line">
        <ErrorLine error={results.error} />
        {results.data?.hits.length === 0 && <div className="p-4 text-muted">Nothing found for this version</div>}
        {results.data?.hits.map((h) => {
          const inPack = have.has(h.project_id) || added.has(h.project_id)
          return (
            <div key={h.project_id} className="flex gap-2.5 border-b border-line px-3 py-2.5">
              <Thumb src={h.icon_url} size={32} />
              <div className="min-w-0 flex-1">
                <div className="flex items-baseline justify-between gap-2">
                  <span className="truncate font-medium">{h.title}</span>
                  <span className="shrink-0 text-[11px] text-[#666]">{count(h.downloads)}</span>
                </div>
                <div className="line-clamp-2 text-xs text-muted">{h.description}</div>
              </div>
              <Button
                small
                className="self-center"
                variant={inPack ? 'ghost' : 'default'}
                disabled={inPack || add.isPending}
                onClick={() => add.mutate({ id: h.project_id, title: h.title })}
              >
                {inPack ? 'Added' : 'Add'}
              </Button>
            </div>
          )
        })}
      </div>
    </aside>
  )
}
