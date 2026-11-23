import { useQuery, useQueryClient } from '@tanstack/react-query'
import { open } from '@tauri-apps/plugin-dialog'
import { useState, type ReactNode } from 'react'
import {
  CheckIcon,
  FolderIcon,
  KeyIcon,
  MonitorIcon,
  MoonIcon,
  RefreshIcon,
  ShieldCheckIcon,
  SunIcon,
  UserIcon,
} from '../components/Icons'
import { PageHeader } from '../components/PageHeader'
import { useUpdate } from '../components/UpdateButton'
import { Button, Field, Input, Tag } from '../components/ui'
import { call } from '../lib/api'
import { keys, useAction } from '../lib/store'
import { useTheme, type ThemeMode } from '../lib/theme'
import type { SettingsForm, SettingsView as Settings } from '../lib/types'

export function SettingsView() {
  const settings = useQuery({ queryKey: keys.settings, queryFn: () => call('get_settings') })
  return (
    <>
      <PageHeader title="Settings" subtitle="Appearance, hub account, updates, and launcher folders" />
      <div className="min-h-0 flex-1 overflow-auto">
        <div className="mx-auto flex max-w-3xl flex-col gap-5 p-6">
          <AppearanceSection />
          {settings.data && <SettingsBody s={settings.data} />}
        </div>
      </div>
    </>
  )
}

function Card({
  icon,
  title,
  description,
  children,
}: {
  icon: ReactNode
  title: string
  description?: string
  children: ReactNode
}) {
  return (
    <section className="overflow-hidden rounded-2xl border border-line bg-surface shadow-theme-xs">
      <div className="flex items-start gap-3 border-b border-line bg-sidebar/40 px-5 py-3.5">
        <div className="mt-0.5 grid size-7 shrink-0 place-items-center rounded-lg border border-line bg-surface text-accent-text shadow-theme-xs">
          {icon}
        </div>
        <div className="min-w-0">
          <h2 className="text-[13.5px] font-semibold tracking-tight text-fg">{title}</h2>
          {description && <p className="text-xs text-muted">{description}</p>}
        </div>
      </div>
      <div className="flex flex-col gap-4 p-5">{children}</div>
    </section>
  )
}

function AppearanceSection() {
  const mode = useTheme((s) => s.mode)
  const setMode = useTheme((s) => s.setMode)

  // Preview swatches mirror the inset shell/canvas light & dark tokens in `src/index.css`.
  const cards: { mode: ThemeMode; label: string; hint: string; icon: ReactNode; preview: ReactNode }[] = [
    {
      mode: 'system',
      label: 'System',
      hint: 'Follow OS setting',
      icon: <MonitorIcon className="size-3.5" />,
      preview: (
        <div className="relative h-14 w-full overflow-hidden rounded-lg border border-line">
          <div className="absolute inset-0 flex">
            <div className="flex w-1/2 bg-[#ebe8e0] p-1">
              <div className="w-1/3 p-1">
                <div className="h-1.5 w-full rounded bg-[#9e5a04]/70" />
                <div className="mt-1 h-1 w-3/4 rounded bg-black/15" />
              </div>
              <div className="flex-1 rounded-md border border-black/10 bg-[#faf9f6] p-1.5">
                <div className="h-1.5 w-2/3 rounded bg-black/15" />
                <div className="mt-1 h-4 rounded border border-black/10 bg-white" />
              </div>
            </div>
            <div className="flex w-1/2 bg-[#08090c] p-1">
              <div className="w-1/3 p-1">
                <div className="h-1.5 w-full rounded bg-[#f5b342]/75" />
                <div className="mt-1 h-1 w-3/4 rounded bg-white/15" />
              </div>
              <div className="flex-1 rounded-md border border-white/10 bg-[#0f1117] p-1.5">
                <div className="h-1.5 w-2/3 rounded bg-white/15" />
                <div className="mt-1 h-4 rounded border border-white/10 bg-[#151822]" />
              </div>
            </div>
          </div>
        </div>
      ),
    },
    {
      mode: 'light',
      label: 'Light',
      hint: 'Warm Quartz studio',
      icon: <SunIcon className="size-3.5" />,
      preview: (
        <div className="flex h-14 w-full overflow-hidden rounded-lg border border-black/10 bg-[#ebe8e0] p-1">
          <div className="w-1/4 p-1">
            <div className="h-1.5 w-full rounded bg-[#9e5a04]/80" />
            <div className="mt-1.5 h-1 w-4/5 rounded bg-black/15" />
            <div className="mt-1 h-1 w-3/5 rounded bg-black/10" />
          </div>
          <div className="flex flex-1 flex-col gap-1.5 rounded-md border border-black/10 bg-[#faf9f6] p-1.5">
            <div className="flex items-center justify-between">
              <div className="h-1.5 w-10 rounded bg-black/20" />
              <div className="h-2.5 w-6 rounded bg-[#9e5a04]" />
            </div>
            <div className="grid flex-1 grid-cols-2 gap-1">
              <div className="rounded border border-black/10 bg-white" />
              <div className="rounded border border-black/10 bg-white" />
            </div>
          </div>
        </div>
      ),
    },
    {
      mode: 'dark',
      label: 'Dark',
      hint: 'Obsidian studio',
      icon: <MoonIcon className="size-3.5" />,
      preview: (
        <div className="flex h-14 w-full overflow-hidden rounded-lg border border-white/10 bg-[#08090c] p-1">
          <div className="w-1/4 p-1">
            <div className="h-1.5 w-full rounded bg-[#f5b342]/80" />
            <div className="mt-1.5 h-1 w-4/5 rounded bg-white/15" />
            <div className="mt-1 h-1 w-3/5 rounded bg-white/10" />
          </div>
          <div className="flex flex-1 flex-col gap-1.5 rounded-md border border-white/10 bg-[#0f1117] p-1.5">
            <div className="flex items-center justify-between">
              <div className="h-1.5 w-10 rounded bg-white/20" />
              <div className="h-2.5 w-6 rounded bg-[#f5b342]" />
            </div>
            <div className="grid flex-1 grid-cols-2 gap-1">
              <div className="rounded border border-white/10 bg-[#151822]" />
              <div className="rounded border border-white/10 bg-[#151822]" />
            </div>
          </div>
        </div>
      ),
    },
  ]

  return (
    <Card icon={<SunIcon className="size-4" />} title="Appearance" description="Choose light, dark, or follow your operating system.">
      <div className="grid grid-cols-3 gap-3">
        {cards.map((c) => {
          const active = mode === c.mode
          return (
            <button
              key={c.mode}
              type="button"
              aria-pressed={active}
              onClick={() => setMode(c.mode)}
              className={`flex flex-col gap-2.5 rounded-xl border p-2.5 text-left transition-all ${
                active
                  ? 'border-accent bg-accent-soft/70 ring-1 ring-accent-border'
                  : 'border-line bg-bg/50 hover:border-line-hover hover:bg-hover'
              }`}
            >
              {c.preview}
              <div className="flex items-center justify-between gap-1 px-0.5">
                <div className="min-w-0">
                  <div className="flex items-center gap-1.5 text-xs font-semibold text-fg">
                    <span className={active ? 'text-accent-text' : 'text-muted'}>{c.icon}</span>
                    {c.label}
                  </div>
                  <div className="truncate text-[11px] text-muted">{c.hint}</div>
                </div>
                {active && (
                  <span className="grid size-4.5 shrink-0 place-items-center rounded-full bg-accent text-accent-fg">
                    <CheckIcon className="size-3" />
                  </span>
                )}
              </div>
            </button>
          )
        })}
      </div>
    </Card>
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
    // The Prism path decides which Prism's accounts are listed.
    invalidate: [keys.settings, keys.library, keys.accounts],
    done: 'Settings saved',
  })
  const browse = async (k: 'instances_dir' | 'prism_path') => {
    const picked = await open({ directory: k === 'instances_dir' })
    if (picked) setForm({ ...form, [k]: picked })
  }

  return (
    <>
      <Card icon={<UserIcon className="size-4" />} title="Account & Hub" description="Sync packs, publish updates, and manage servers.">
        <Field label="Hub URL">{(id) => <Input id={id} {...field('hub')} placeholder={s.hub} />}</Field>
        {s.signed_in ? <SignedIn username={s.username ?? ''} /> : <SignIn prepare={() => call('save_settings', { form })} />}
      </Card>

      {s.signed_in && (
        <Card
          icon={<ShieldCheckIcon className="size-4" />}
          title="Approve a device"
          description="Authorize a CLI or server machine running beacon login."
        >
          <ApproveDevice />
        </Card>
      )}

      <Card icon={<RefreshIcon className="size-4" />} title="Updates" description="BeaconSync checks GitHub releases automatically in the background.">
        <Updates />
      </Card>

      <Card icon={<KeyIcon className="size-4" />} title="CurseForge" description="Optional API key to search and install CurseForge modpacks.">
        <Field label="API key" hint="Get one at console.curseforge.com">
          {(id) => <Input id={id} {...field('curseforge_key')} type="password" placeholder="Not set" />}
        </Field>
      </Card>

      <Card icon={<FolderIcon className="size-4" />} title="Folders" description="Where packs live and which Prism Launcher starts Minecraft.">
        <Field label="Instances folder">
          {(id) => (
            <div className="flex gap-2">
              <Input id={id} {...field('instances_dir')} placeholder={s.instances_dir} className="flex-1" />
              <Button onClick={() => browse('instances_dir')}>Browse</Button>
            </div>
          )}
        </Field>
        <Field label="Prism Launcher" hint={PRISM_HINT[s.prism]}>
          {(id) => (
            <div className="flex gap-2">
              <Input id={id} {...field('prism_path')} placeholder="Use installed, else built in" className="flex-1" />
              <Button onClick={() => browse('prism_path')}>Browse</Button>
            </div>
          )}
        </Field>
      </Card>

      <div className="flex items-center justify-end pb-4">
        <Button variant="primary" onClick={() => save.mutate(undefined)} disabled={save.isPending}>
          Save settings
        </Button>
      </div>
    </>
  )
}

/** The running version and a manual check. The background one runs every 10 to 15 minutes;
 * a newer version shows up as the button next to the logo either way. */
function Updates() {
  const status = useUpdate()
  const qc = useQueryClient()
  const check = useAction(() => call('update_check'), {
    label: 'Checking for updates',
    done: (v) => (v.offer ? `BeaconSync ${v.offer.version} is available` : 'BeaconSync is up to date'),
    onSuccess: (v) => qc.setQueryData(keys.update, v),
  })
  return (
    <div className="flex items-center gap-3 rounded-xl border border-line bg-bg/60 px-4 py-3">
      <span className="text-[13px] text-fg-secondary">
        BeaconSync <span className="font-mono font-semibold text-fg">{status?.current}</span>
      </span>
      {status?.offer ? (
        <Tag tone="accent" dot>
          {status.offer.version} available
        </Tag>
      ) : (
        status?.latest && (
          <Tag tone="ok" dot>
            Up to date
          </Tag>
        )
      )}
      <Button small className="ml-auto" disabled={check.isPending} onClick={() => check.mutate(undefined)}>
        <RefreshIcon className="size-3.5" />
        Check for updates
      </Button>
    </div>
  )
}

function SignedIn({ username }: { username: string }) {
  const out = useAction(() => call('sign_out'), { label: 'Signing out', invalidate: [keys.settings] })
  return (
    <div className="flex items-center justify-between gap-3 rounded-xl border border-line bg-bg/60 px-4 py-3">
      <div className="flex items-center gap-2.5">
        <span className="grid size-7 place-items-center rounded-full bg-accent-soft text-xs font-semibold uppercase text-accent-text ring-1 ring-accent-border">
          {username.slice(0, 1) || '?'}
        </span>
        <div className="text-[13px]">
          <span className="text-muted">Signed in as </span>
          <span className="font-semibold text-fg">{username}</span>
        </div>
      </div>
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
      className="flex flex-col gap-3 rounded-xl border border-line bg-bg/60 p-4"
      onSubmit={(e) => {
        e.preventDefault()
        if (ready) signIn.mutate(false)
      }}
    >
      <div className="grid grid-cols-2 gap-2.5">
        <Input value={username} onChange={(e) => setUsername(e.target.value)} placeholder="Username" autoComplete="username" />
        <Input
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder="Password"
          type="password"
          autoComplete="current-password"
        />
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
      {(id) => (
        <div className="flex gap-2">
          <Input
            id={id}
            value={code}
            onChange={(e) => setCode(e.target.value.toUpperCase())}
            placeholder="ABCD-1234"
            className="flex-1 font-mono tracking-wider"
          />
          <Button variant="primary" disabled={!code.trim() || approve.isPending} onClick={() => approve.mutate(undefined)}>
            Approve
          </Button>
        </div>
      )}
    </Field>
  )
}

const PRISM_HINT: Record<Settings['prism'], ReactNode> = {
  installed: (
    <Tag tone="ok" dot>
      Using your Prism Launcher
    </Tag>
  ),
  built_in: (
    <Tag tone="ok" dot>
      Built in · downloaded on first play
    </Tag>
  ),
  missing: (
    <Tag tone="warn" dot>
      No built-in Prism for this computer · install from prismlauncher.org
    </Tag>
  ),
}
