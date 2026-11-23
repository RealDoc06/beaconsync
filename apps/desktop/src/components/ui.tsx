// Shared UI primitives styled through BeaconSync's semantic dual-theme tokens.

import {
  useEffect,
  useId,
  useRef,
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
  type TextareaHTMLAttributes,
} from 'react'
import { createPortal } from 'react-dom'
import { AlertIcon, CloseIcon } from './Icons'

type Variant = 'primary' | 'default' | 'ghost' | 'ghost-danger' | 'danger'
type ButtonSize = 'sm' | 'md' | 'lg' | 'icon'

const variants: Record<Variant, string> = {
  primary:
    'border-accent bg-accent text-accent-fg shadow-theme-accent hover:bg-accent-hover hover:border-accent-hover',
  default:
    'border-line-strong bg-surface text-fg shadow-theme-xs hover:bg-surface-2 hover:border-line-hover',
  ghost:
    'border-transparent bg-transparent text-muted hover:bg-hover hover:text-fg',
  'ghost-danger':
    'border-transparent bg-transparent text-muted hover:bg-bad-soft hover:text-bad',
  danger:
    'border-bad-border bg-bad-soft text-bad hover:bg-bad/20 hover:border-bad/50',
}

const buttonSizes: Record<ButtonSize, string> = {
  sm: 'h-7 px-2.5 text-xs',
  md: 'h-8.5 px-3 text-[13px]',
  lg: 'h-10 px-4 text-sm',
  icon: 'size-7 px-0 text-xs',
}

export function Button({
  variant = 'default',
  size,
  small,
  className = '',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: Variant
  size?: ButtonSize
  small?: boolean
}) {
  const resolvedSize: ButtonSize = size ?? (small ? 'sm' : 'md')
  return (
    <button
      type="button"
      {...props}
      className={`inline-flex shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded-lg border font-medium transition-all duration-150 enabled:active:scale-[0.98] disabled:opacity-40 ${buttonSizes[resolvedSize]} ${variants[variant]} ${className}`}
    />
  )
}

type InputSize = 'sm' | 'md' | 'lg'

const inputSizes: Record<InputSize, string> = {
  sm: 'h-7 px-2 text-xs',
  md: 'h-8.5 px-3 text-[13px]',
  lg: 'h-10 px-3 text-sm',
}

export function Input({
  className = '',
  icon,
  inputSize = 'md',
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { icon?: ReactNode; inputSize?: InputSize }) {
  const base = `min-w-0 rounded-lg border border-line-strong bg-input text-fg shadow-theme-xs outline-none transition-colors placeholder:text-subtle hover:border-line-hover focus:border-accent focus-visible:ring-2 focus-visible:ring-ring ${inputSizes[inputSize]}`
  if (!icon) {
    return <input {...props} className={`${base} ${className}`} />
  }
  return (
    <div className={`relative flex items-center ${className}`}>
      <span className="pointer-events-none absolute left-2.5 flex items-center text-subtle">{icon}</span>
      <input {...props} className={`${base} w-full pl-8`} />
    </div>
  )
}

export function Textarea({ className = '', ...props }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return (
    <textarea
      {...props}
      className={`min-w-0 rounded-lg border border-line-strong bg-input p-3 text-[13px] text-fg shadow-theme-xs outline-none transition-colors placeholder:text-subtle hover:border-line-hover focus:border-accent focus-visible:ring-2 focus-visible:ring-ring ${className}`}
    />
  )
}

type SelectSize = 'sm' | 'md'

const selectSizes: Record<SelectSize, string> = {
  sm: 'h-7.5 px-2 text-xs',
  md: 'h-8.5 px-2.5 text-[13px]',
}

export function Select<V extends string>({
  id,
  value,
  onChange,
  options,
  size = 'md',
  className = '',
  disabled,
  'aria-label': ariaLabel,
}: {
  id?: string
  value: V
  onChange: (v: V) => void
  options: readonly { value: V; label: string }[]
  size?: SelectSize
  className?: string
  disabled?: boolean
  'aria-label'?: string
}) {
  return (
    <select
      id={id}
      value={value}
      disabled={disabled}
      aria-label={ariaLabel}
      onChange={(e) => onChange(e.target.value as V)}
      className={`rounded-lg border border-line-strong bg-input text-fg shadow-theme-xs outline-none transition-colors hover:border-line-hover focus:border-accent focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-40 ${selectSizes[size]} ${className}`}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  )
}

/** Form field wrapper with explicit `htmlFor`/`id` association so composite rows (e.g. input +
 * browse button) never nest interactive buttons inside `<label>`. Pass `group` for multi-button
 * controls like `Segmented` to expose `role="group"` with `aria-labelledby`. */
export function Field({
  label,
  children,
  hint,
  group,
}: {
  label: string
  children: ReactNode | ((id: string) => ReactNode)
  hint?: ReactNode
  group?: boolean
}) {
  const id = useId()
  if (group) {
    return (
      <div role="group" aria-labelledby={id} className="flex flex-col gap-1.5">
        <span id={id} className="text-xs font-medium text-fg-secondary">
          {label}
        </span>
        {typeof children === 'function' ? children(id) : children}
        {hint && <span className="text-xs text-muted">{hint}</span>}
      </div>
    )
  }
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={id} className="self-start text-xs font-medium text-fg-secondary">
        {label}
      </label>
      {typeof children === 'function' ? children(id) : children}
      {hint && <span className="text-xs text-muted">{hint}</span>}
    </div>
  )
}

export function Toggle({
  checked,
  onChange,
  label,
  ariaLabel,
  disabled,
}: {
  checked: boolean
  onChange: (v: boolean) => void
  label?: string
  ariaLabel?: string
  disabled?: boolean
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel ?? label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className="inline-flex items-center gap-2.5 rounded-full disabled:opacity-40"
    >
      <span
        className={`relative h-[18px] w-8 rounded-full border transition-colors duration-150 ${
          checked ? 'border-accent bg-accent' : 'border-line-strong bg-surface-2'
        }`}
      >
        <span
          className={`absolute top-1/2 left-0 size-3 -translate-y-1/2 rounded-full transition-transform duration-150 ${
            checked ? 'translate-x-[16px] bg-accent-fg' : 'translate-x-[2px] bg-muted'
          }`}
        />
      </span>
      {label && <span className="text-xs font-medium text-fg-secondary">{label}</span>}
    </button>
  )
}

/** Studio pill navigation bar with roving tabindex and arrow-key navigation. */
export function Tabs<V extends string>({
  value,
  onChange,
  tabs,
  variant = 'pill',
}: {
  value: V
  onChange: (v: V) => void
  tabs: readonly { value: V; label: ReactNode; icon?: ReactNode }[]
  variant?: 'pill' | 'underline'
}) {
  const listRef = useRef<HTMLDivElement>(null)
  const onKeyDown = (e: React.KeyboardEvent, idx: number) => {
    let next = -1
    if (e.key === 'ArrowRight') next = (idx + 1) % tabs.length
    else if (e.key === 'ArrowLeft') next = (idx - 1 + tabs.length) % tabs.length
    else if (e.key === 'Home') next = 0
    else if (e.key === 'End') next = tabs.length - 1
    if (next < 0) return
    e.preventDefault()
    const target = tabs[next]
    if (!target) return
    onChange(target.value)
    listRef.current?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus()
  }

  if (variant === 'underline') {
    return (
      <div ref={listRef} role="tablist" className="flex items-center gap-1 border-b border-line px-6">
        {tabs.map((t, idx) => {
          const active = value === t.value
          return (
            <button
              key={t.value}
              type="button"
              role="tab"
              aria-selected={active}
              tabIndex={active ? 0 : -1}
              onClick={() => onChange(t.value)}
              onKeyDown={(e) => onKeyDown(e, idx)}
              className={`relative -mb-px flex h-10 items-center gap-2 px-3 text-[13px] font-medium transition-colors ${
                active ? 'text-fg' : 'text-muted hover:text-fg-secondary'
              }`}
            >
              {t.icon && <span className={active ? 'text-accent-text' : 'text-muted'}>{t.icon}</span>}
              {t.label}
              {active && <span className="absolute right-2 bottom-0 left-2 h-[2px] rounded-full bg-accent" />}
            </button>
          )
        })}
      </div>
    )
  }

  return (
    <div className="flex items-center border-b border-line px-6 py-2.5">
      <div ref={listRef} role="tablist" className="inline-flex items-center gap-1 rounded-xl border border-line bg-sidebar p-1">
        {tabs.map((t, idx) => {
          const active = value === t.value
          return (
            <button
              key={t.value}
              type="button"
              role="tab"
              aria-selected={active}
              tabIndex={active ? 0 : -1}
              onClick={() => onChange(t.value)}
              onKeyDown={(e) => onKeyDown(e, idx)}
              className={`inline-flex h-7.5 items-center gap-1.5 rounded-lg px-3 text-[12.5px] font-medium transition-all ${
                active
                  ? 'bg-surface text-fg shadow-theme-xs ring-1 ring-line-strong'
                  : 'text-muted hover:text-fg'
              }`}
            >
              {t.icon && <span className={active ? 'text-accent-text' : 'text-muted'}>{t.icon}</span>}
              {t.label}
            </button>
          )
        })}
      </div>
    </div>
  )
}

/** Tactile segmented control for filters and mode switches. */
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
    <div className="inline-flex h-8.5 items-center rounded-lg border border-line bg-sidebar p-0.5">
      {options.map((o) => {
        const active = value === o.value
        return (
          <button
            key={o.value}
            type="button"
            aria-pressed={active}
            onClick={() => onChange(o.value)}
            className={`inline-flex h-full items-center gap-1.5 whitespace-nowrap rounded-md px-2.5 text-xs font-medium transition-all ${
              active
                ? 'bg-surface text-fg shadow-theme-xs ring-1 ring-line-strong'
                : 'text-muted hover:text-fg-secondary'
            }`}
          >
            {o.label}
          </button>
        )
      })}
    </div>
  )
}

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'

/** Stack of mounted dialog elements so only the topmost dialog intercepts Escape and Tab. */
const dialogStack: HTMLElement[] = []

export function Dialog({
  title,
  subtitle,
  onClose,
  children,
  footer,
  wide,
}: {
  title: string
  subtitle?: string
  onClose: () => void
  children: ReactNode
  footer?: ReactNode
  wide?: boolean
}) {
  const ref = useRef<HTMLDivElement>(null)
  const closeRef = useRef(onClose)
  closeRef.current = onClose

  useEffect(() => {
    const dialog = ref.current
    if (!dialog) return
    const prev = document.activeElement instanceof HTMLElement ? document.activeElement : null
    dialogStack.push(dialog)
    const initial = dialog.querySelector<HTMLElement>('input:not([disabled]),textarea:not([disabled]),select:not([disabled])')
    ;(initial ?? dialog).focus()

    const onKey = (e: KeyboardEvent) => {
      if (dialogStack[dialogStack.length - 1] !== dialog) return
      if (e.key === 'Escape') {
        e.preventDefault()
        closeRef.current()
        return
      }
      if (e.key !== 'Tab') return
      const nodes = [...dialog.querySelectorAll<HTMLElement>(FOCUSABLE)]
      if (!nodes.length) {
        e.preventDefault()
        dialog.focus()
        return
      }
      const first = nodes[0]!
      const last = nodes[nodes.length - 1]!
      const active = document.activeElement
      if (e.shiftKey) {
        if (active === first || active === dialog || !dialog.contains(active)) {
          e.preventDefault()
          last.focus()
        }
      } else if (active === last || active === dialog || !dialog.contains(active)) {
        e.preventDefault()
        first.focus()
      }
    }

    window.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener('keydown', onKey)
      const isTop = dialogStack[dialogStack.length - 1] === dialog
      const idx = dialogStack.lastIndexOf(dialog)
      if (idx !== -1) dialogStack.splice(idx, 1)
      if (isTop && prev?.isConnected) prev.focus()
    }
  }, [])

  return createPortal(
    <div
      className="animate-fade fixed inset-0 z-50 flex items-center justify-center bg-overlay p-4"
      onMouseDown={() => closeRef.current()}
    >
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        onMouseDown={(e) => e.stopPropagation()}
        className={`animate-dialog flex max-h-[85vh] w-full flex-col overflow-hidden rounded-2xl border border-line-strong bg-surface shadow-theme-lg outline-none ${
          wide ? 'max-w-3xl' : 'max-w-md'
        }`}
      >
        <div className="flex shrink-0 items-start justify-between gap-3 border-b border-line px-5 py-3.5">
          <div className="min-w-0">
            <h2 className="break-words text-[15px] font-semibold leading-snug tracking-tight text-fg">{title}</h2>
            {subtitle && <p className="mt-0.5 text-xs leading-relaxed text-muted">{subtitle}</p>}
          </div>
          <button
            type="button"
            onClick={() => closeRef.current()}
            className="grid size-7 shrink-0 place-items-center rounded-lg text-muted transition-colors hover:bg-hover hover:text-fg"
            aria-label="Close"
          >
            <CloseIcon className="size-3.5" />
          </button>
        </div>
        <div className="min-h-0 overflow-auto p-5">{children}</div>
        {footer && (
          <div className="flex shrink-0 items-center justify-end gap-2 border-t border-line bg-sidebar/60 px-5 py-3">
            {footer}
          </div>
        )}
      </div>
    </div>,
    document.body,
  )
}

/** Progress bar driven by backend progress events. */
export function ProgressBar({ done, total }: { done: number; total: number }) {
  const pct = total > 0 ? Math.min(100, (done / total) * 100) : 0
  return (
    <div className="h-1.5 w-44 overflow-hidden rounded-full bg-surface-2 ring-1 ring-line">
      <div className="h-full rounded-full bg-accent transition-[width] duration-200" style={{ width: `${pct}%` }} />
    </div>
  )
}

export type TagTone = 'muted' | 'accent' | 'info' | 'warn' | 'ok' | 'bad'

const tagStyles: Record<TagTone, string> = {
  muted: 'border-line bg-surface-2/80 text-muted',
  accent: 'border-accent-border bg-accent-soft text-accent-text',
  info: 'border-info-border bg-info-soft text-info',
  warn: 'border-warn-border bg-warn-soft text-warn',
  ok: 'border-ok-border bg-ok-soft text-ok',
  bad: 'border-bad-border bg-bad-soft text-bad',
}

/** Compact tinted status pill. */
export function Tag({
  tone = 'muted',
  dot,
  shrink,
  className = '',
  children,
}: {
  tone?: TagTone
  dot?: boolean
  shrink?: boolean
  className?: string
  children: ReactNode
}) {
  const dotColor: Record<TagTone, string> = {
    muted: 'bg-muted',
    accent: 'bg-accent',
    info: 'bg-info',
    warn: 'bg-warn',
    ok: 'bg-ok',
    bad: 'bg-bad',
  }
  return (
    <span
      className={`inline-flex items-center gap-1.5 whitespace-nowrap rounded-full border px-2 py-0.5 text-[11px] font-medium leading-tight ${
        shrink ? 'min-w-0 shrink overflow-hidden' : 'shrink-0'
      } ${tagStyles[tone]} ${className}`}
    >
      {dot && <span className={`size-1.5 rounded-full ${dotColor[tone]}`} />}
      {children}
    </span>
  )
}

export function Empty({
  icon,
  title,
  children,
  action,
}: {
  icon?: ReactNode
  title?: string
  children: ReactNode
  action?: ReactNode
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 px-6 py-16 text-center">
      {icon && (
        <div className="grid size-11 place-items-center rounded-2xl border border-line bg-surface text-muted shadow-theme-xs">
          {icon}
        </div>
      )}
      <div className="flex max-w-sm flex-col gap-1">
        {title && <div className="text-sm font-semibold text-fg">{title}</div>}
        <div className="text-[13px] text-muted">{children}</div>
      </div>
      {action && <div className="mt-1">{action}</div>}
    </div>
  )
}

export function ErrorLine({ error, className = '' }: { error: unknown; className?: string }) {
  if (!error) return null
  return (
    <div
      role="alert"
      className={`flex items-center gap-2 rounded-lg border border-bad-border bg-bad-soft px-3 py-2 text-xs text-bad ${className}`}
    >
      <AlertIcon className="size-3.5 shrink-0" />
      <span className="min-w-0 break-words">{error instanceof Error ? error.message : String(error)}</span>
    </div>
  )
}

/** Lazy loaded square project icon with a neutral framed fallback. */
export function Thumb({ src, size = 32 }: { src: string | null | undefined; size?: number }) {
  return src ? (
    <img
      src={src}
      loading="lazy"
      decoding="async"
      width={size}
      height={size}
      className="shrink-0 rounded-lg border border-line bg-surface-2 object-cover shadow-theme-xs"
      style={{ width: size, height: size }}
      alt=""
    />
  ) : (
    <div className="shrink-0 rounded-lg border border-line bg-surface-2" style={{ width: size, height: size }} />
  )
}
