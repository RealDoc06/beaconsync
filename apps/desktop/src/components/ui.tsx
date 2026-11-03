// Small UI primitives. No component library: plain elements with Tailwind classes.

import { useEffect, useRef, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode } from 'react'

type Variant = 'primary' | 'default' | 'ghost' | 'danger'

const variants: Record<Variant, string> = {
  primary: 'bg-fg text-bg hover:bg-white/85 border-fg',
  default: 'border-line-strong hover:bg-hover hover:border-[#444]',
  ghost: 'border-transparent hover:bg-hover text-muted hover:text-fg',
  danger: 'border-line-strong text-bad hover:bg-bad/10 hover:border-bad/40',
}

export function Button({
  variant = 'default',
  small,
  className = '',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; small?: boolean }) {
  return (
    <button
      type="button"
      {...props}
      className={`inline-flex shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded-md border font-medium transition-colors disabled:pointer-events-none disabled:opacity-40 ${
        small ? 'h-6 px-2 text-xs' : 'h-8 px-3'
      } ${variants[variant]} ${className}`}
    />
  )
}

export function Input({ className = '', ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      {...props}
      className={`h-8 min-w-0 rounded-md border border-line-strong bg-bg px-2.5 outline-none placeholder:text-[#555] focus:border-[#666] ${className}`}
    />
  )
}

export function Select<V extends string>({
  value,
  onChange,
  options,
  className = '',
  disabled,
}: {
  value: V
  onChange: (v: V) => void
  options: readonly { value: V; label: string }[]
  className?: string
  disabled?: boolean
}) {
  return (
    <select
      value={value}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value as V)}
      className={`h-8 rounded-md border border-line-strong bg-bg px-2 outline-none focus:border-[#666] disabled:opacity-40 ${className}`}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  )
}

export function Field({ label, children, hint }: { label: string; children: ReactNode; hint?: ReactNode }) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-xs font-medium">{label}</span>
      {children}
      {hint && <span className="text-xs text-muted">{hint}</span>}
    </label>
  )
}

export function Toggle({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean
  onChange: (v: boolean) => void
  label?: string
  disabled?: boolean
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className="inline-flex items-center gap-2 disabled:opacity-40"
    >
      <span
        className={`relative h-4 w-7 rounded-full border transition-colors ${checked ? 'border-accent bg-accent' : 'border-line-strong bg-[#151515]'}`}
      >
        <span
          className={`absolute top-1/2 size-2.5 -translate-y-1/2 rounded-full transition-[left] ${checked ? 'left-[14px] bg-bg' : 'left-[2px] bg-muted'}`}
        />
      </span>
      {label && <span className="text-xs">{label}</span>}
    </button>
  )
}

/** Underlined tab strip. */
export function Tabs<V extends string>({
  value,
  onChange,
  tabs,
}: {
  value: V
  onChange: (v: V) => void
  tabs: readonly { value: V; label: ReactNode }[]
}) {
  return (
    <div className="flex gap-5 border-b border-line px-5">
      {tabs.map((t) => (
        <button
          key={t.value}
          type="button"
          onClick={() => onChange(t.value)}
          className={`-mb-px flex h-9 items-center gap-1.5 border-b-2 text-[13px] ${
            value === t.value ? 'border-fg text-fg' : 'border-transparent text-muted hover:text-fg'
          }`}
        >
          {t.label}
        </button>
      ))}
    </div>
  )
}

/** Compact segmented control for filters. */
export function Segmented<V extends string>({
  value,
  onChange,
  options,
}: {
  value: V
  onChange: (v: V) => void
  options: readonly { value: V; label: ReactNode }[]
}) {
  return (
    <div className="inline-flex h-8 rounded-md border border-line-strong p-0.5">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          onClick={() => onChange(o.value)}
          className={`whitespace-nowrap rounded px-2.5 text-xs ${value === o.value ? 'bg-[#1c1c1c] text-fg' : 'text-muted hover:text-fg'}`}
        >
          {o.label}
        </button>
      ))}
    </div>
  )
}

export function Dialog({
  title,
  onClose,
  children,
  footer,
  wide,
}: {
  title: string
  onClose: () => void
  children: ReactNode
  footer?: ReactNode
  wide?: boolean
}) {
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKey)
    ref.current?.querySelector<HTMLElement>('input,textarea,select')?.focus()
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70" onMouseDown={onClose}>
      <div
        ref={ref}
        role="dialog"
        aria-label={title}
        onMouseDown={(e) => e.stopPropagation()}
        className={`flex max-h-[85vh] w-full flex-col rounded-lg border border-line-strong bg-bg ${wide ? 'max-w-3xl' : 'max-w-md'}`}
      >
        <div className="flex h-11 shrink-0 items-center justify-between border-b border-line px-4">
          <h2 className="font-semibold">{title}</h2>
          <button type="button" onClick={onClose} className="text-muted hover:text-fg" aria-label="Close">
            ✕
          </button>
        </div>
        <div className="min-h-0 overflow-auto p-4">{children}</div>
        {footer && <div className="flex shrink-0 justify-end gap-2 border-t border-line px-4 py-3">{footer}</div>}
      </div>
    </div>
  )
}

/** Static progress bar driven by progress events (no animation). */
export function ProgressBar({ done, total }: { done: number; total: number }) {
  const pct = total > 0 ? Math.min(100, (done / total) * 100) : 0
  return (
    <div className="h-1 w-40 overflow-hidden rounded-full bg-[#1a1a1a]">
      <div className="h-full bg-accent" style={{ width: `${pct}%` }} />
    </div>
  )
}

/** Small inline label, e.g. "3 unsaved". Text colored, no pill chrome. */
export function Tag({ tone = 'muted', children }: { tone?: 'muted' | 'accent' | 'warn' | 'ok' | 'bad'; children: ReactNode }) {
  const color = { muted: 'text-muted', accent: 'text-accent', warn: 'text-warn', ok: 'text-ok', bad: 'text-bad' }[tone]
  return <span className={`text-xs whitespace-nowrap ${color}`}>{children}</span>
}

export function Empty({ children, action }: { children: ReactNode; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 py-16 text-muted">
      <div>{children}</div>
      {action}
    </div>
  )
}

export function ErrorLine({ error }: { error: unknown }) {
  if (!error) return null
  return <div className="text-xs text-bad">{error instanceof Error ? error.message : String(error)}</div>
}

/** Lazy loaded square project icon with a neutral fallback. */
export function Thumb({ src, size = 32 }: { src: string | null | undefined; size?: number }) {
  return src ? (
    <img src={src} loading="lazy" decoding="async" width={size} height={size} className="shrink-0 rounded bg-[#111] object-cover" style={{ width: size, height: size }} alt="" />
  ) : (
    <div className="shrink-0 rounded bg-[#151515]" style={{ width: size, height: size }} />
  )
}
