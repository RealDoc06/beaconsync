import { useQuery } from '@tanstack/react-query'
import { open } from '@tauri-apps/plugin-dialog'
import { useState, type ReactNode } from 'react'
import { PageHeader } from '../components/PageHeader'
import { Button, Field, Input, Tag } from '../components/ui'
import { call } from '../lib/api'
import { keys, useAction } from '../lib/store'
import type { SettingsForm, SettingsView as Settings } from '../lib/types'

export function SettingsView() {
  const settings = useQuery({ queryKey: keys.settings, queryFn: () => call('get_settings') })
  return (
    <>
      <PageHeader title="Settings" />
      <div className="min-h-0 flex-1 overflow-auto">{settings.data && <SettingsBody s={settings.data} />}</div>
    </>
  )
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="grid grid-cols-[200px_minmax(0,460px)] gap-6 border-b border-line px-5 py-5">
      <h2 className="font-semibold">{title}</h2>
      <div className="flex flex-col gap-3">{children}</div>
    </section>
  )
}

function SettingsBody({ s }: { s: Settings }) {
  const [form, setForm] = useState<SettingsForm>({
    hub: s.hub_custom ? s.hub : '',
    curseforge_key: s.curseforge_key,
    instances_dir: s.instances_custom ? s.instances_dir : '',
    prism_path: s.prism_path ?? '',
  })
  const field = (k: keyof SettingsForm) => ({
    value: form[k],
    onChange: (e: React.ChangeEvent<HTMLInputElement>) => setForm({ ...form, [k]: e.target.value }),
  })
  const save = useAction(() => call('save_settings', { form }), {
    label: 'Saving settings',
    invalidate: [keys.settings, keys.library],
    done: 'Settings saved',
  })
  const browse = async (k: 'instances_dir' | 'prism_path') => {
    const picked = await open({ directory: k === 'instances_dir' })
    if (picked) setForm({ ...form, [k]: picked })
  }

  return (
    <>
      <Section title="Account">
        <Field label="Hub">
          <Input {...field('hub')} placeholder={s.hub} />
        </Field>
        {s.signed_in ? <SignedIn username={s.username ?? ''} /> : <SignIn prepare={() => call('save_settings', { form })} />}
      </Section>
      {s.signed_in && (
        <Section title="Approve a device">
          <ApproveDevice />
        </Section>
      )}
      <Section title="CurseForge">
        <Field label="API key" hint="Needed for CurseForge modpacks. Get one at console.curseforge.com">
          <Input {...field('curseforge_key')} type="password" placeholder="Not set" />
        </Field>
      </Section>
      <Section title="Folders">
        <Field label="Instances folder">
          <div className="flex gap-2">
            <Input {...field('instances_dir')} placeholder={s.instances_dir} className="flex-1" />
            <Button onClick={() => browse('instances_dir')}>Browse</Button>
          </div>
        </Field>
        <Field
          label="Prism Launcher"
          hint={s.prism_found ? <Tag tone="ok">Prism Launcher found</Tag> : <Tag tone="warn">Prism Launcher not found. Install it from prismlauncher.org to play.</Tag>}
        >
          <div className="flex gap-2">
            <Input {...field('prism_path')} placeholder="Detect automatically" className="flex-1" />
            <Button onClick={() => browse('prism_path')}>Browse</Button>
          </div>
        </Field>
      </Section>
      <div className="px-5 py-4">
        <Button variant="primary" onClick={() => save.mutate(undefined)} disabled={save.isPending}>
          Save settings
        </Button>
      </div>
    </>
  )
}

function SignedIn({ username }: { username: string }) {
  const out = useAction(() => call('sign_out'), { label: 'Signing out', invalidate: [keys.settings] })
  return (
    <div className="flex items-center gap-3">
      <span>
        Signed in as <span className="font-medium">{username}</span>
      </span>
      <Button small onClick={() => out.mutate(undefined)}>
        Sign out
      </Button>
    </div>
  )
}

/** `prepare` saves the form first so signing in uses the hub typed above. */
function SignIn({ prepare }: { prepare: () => Promise<unknown> }) {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const signIn = useAction(
    async (create: boolean) => {
      await prepare()
      return call('sign_in', { username, password, create })
    },
    {
      label: 'Signing in',
      invalidate: [keys.settings],
      done: (u) => `Signed in as ${u}`,
    },
  )
  const ready = username.trim() !== '' && password !== '' && !signIn.isPending
  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault()
        if (ready) signIn.mutate(false)
      }}
    >
      <div className="grid grid-cols-2 gap-2">
        <Input value={username} onChange={(e) => setUsername(e.target.value)} placeholder="Username" autoComplete="username" />
        <Input value={password} onChange={(e) => setPassword(e.target.value)} placeholder="Password" type="password" autoComplete="current-password" />
      </div>
      <div className="flex gap-2">
        <Button type="submit" variant="primary" disabled={!ready}>
          Sign in
        </Button>
        <Button disabled={!ready} onClick={() => signIn.mutate(true)}>
          Create account
        </Button>
      </div>
    </form>
  )
}

function ApproveDevice() {
  const [code, setCode] = useState('')
  const approve = useAction(() => call('approve_device', { code }), {
    label: 'Approving device',
    done: 'Device approved',
    onSuccess: () => setCode(''),
  })
  return (
    <Field label="Code shown by beacon login">
      <div className="flex gap-2">
        <Input value={code} onChange={(e) => setCode(e.target.value.toUpperCase())} placeholder="ABCD-1234" className="flex-1 font-mono" />
        <Button disabled={!code.trim() || approve.isPending} onClick={() => approve.mutate(undefined)}>
          Approve
        </Button>
      </div>
    </Field>
  )
}
