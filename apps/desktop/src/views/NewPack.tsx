import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Button, Dialog, ErrorLine, Field, Input, Segmented, Select, Toggle } from '../components/ui'
import { call } from '../lib/api'
import { LOADER_LABEL } from '../lib/format'
import { keys, useAction, useApp } from '../lib/store'
import type { LoaderKind } from '../lib/types'

const LOADERS: readonly { value: LoaderKind; label: string }[] = [
  { value: 'vanilla', label: 'None' },
  ...(['fabric', 'quilt', 'forge', 'neoforge'] as const).map((k) => ({ value: k, label: LOADER_LABEL[k] })),
]

export function NewPack() {
  const open = useApp((s) => s.newPackOpen)
  const set = useApp((s) => s.set)
  return open ? <NewPackDialog onClose={() => set({ newPackOpen: false })} /> : null
}

function NewPackDialog({ onClose }: { onClose: () => void }) {
  const go = useApp((s) => s.go)
  const [name, setName] = useState('')
  const [snapshots, setSnapshots] = useState(false)
  const [mc, setMc] = useState('')
  const [loader, setLoader] = useState<LoaderKind>('fabric')
  const [loaderVersion, setLoaderVersion] = useState('')

  const versions = useQuery({ queryKey: ['mc-versions'], queryFn: () => call('minecraft_versions'), staleTime: Infinity })
  const shown = (versions.data ?? []).filter((v) => v.type === 'release' || (snapshots && v.type === 'snapshot'))
  const minecraft = mc || shown[0]?.id || ''

  const loaders = useQuery({
    queryKey: ['loader-versions', loader, minecraft],
    queryFn: () => call('loader_versions', { kind: loader, minecraft }),
    enabled: loader !== 'vanilla' && minecraft !== '',
    staleTime: Infinity,
  })
  // Stable versions first; the newest stable is the default.
  const loaderOptions = [...(loaders.data ?? [])].sort((a, b) => Number(b.stable) - Number(a.stable))
  const chosenLoader = loaderVersion && loaderOptions.some((v) => v.version === loaderVersion) ? loaderVersion : loaderOptions[0]?.version

  const create = useAction(
    () =>
      call('create_pack', {
        name: name.trim(),
        minecraft,
        loader: loader === 'vanilla' ? { kind: 'vanilla' } : { kind: loader, version: chosenLoader },
      }),
    {
      label: 'Creating pack',
      invalidate: [keys.library],
      onSuccess: (path) => {
        onClose()
        go({ name: 'pack', path, tab: 'mods' })
      },
    },
  )
  const missingLoader = loader !== 'vanilla' && !chosenLoader
  const canCreate = name.trim() !== '' && minecraft !== '' && !missingLoader && !create.isPending

  return (
    <Dialog
      title="New pack"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!canCreate} onClick={() => create.mutate(undefined)}>
            Create
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <Field label="Name">
          <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="My pack" />
        </Field>
        <Field label="Minecraft version">
          <div className="flex items-center gap-3">
            <Select
              className="flex-1"
              value={minecraft}
              onChange={setMc}
              disabled={!versions.data}
              options={shown.length ? shown.map((v) => ({ value: v.id, label: v.id })) : [{ value: '', label: 'Loading versions' }]}
            />
            <Toggle checked={snapshots} onChange={setSnapshots} label="Snapshots" />
          </div>
          <ErrorLine error={versions.error} />
        </Field>
        <Field label="Mod loader">
          <Segmented value={loader} onChange={setLoader} options={LOADERS} />
        </Field>
        {loader !== 'vanilla' && (
          <Field label={`${LOADER_LABEL[loader]} version`}>
            <Select
              value={chosenLoader ?? ''}
              onChange={setLoaderVersion}
              disabled={!loaderOptions.length}
              options={
                loaderOptions.length
                  ? loaderOptions.map((v) => ({ value: v.version, label: v.stable ? v.version : `${v.version} (beta)` }))
                  : [{ value: '', label: loaders.isFetching ? 'Loading' : `No ${LOADER_LABEL[loader]} for ${minecraft}` }]
              }
            />
            <ErrorLine error={loaders.error} />
          </Field>
        )}
      </div>
    </Dialog>
  )
}
