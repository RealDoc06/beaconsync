import type { ReactNode } from 'react'

/** Top header inside the inset workspace canvas. */
export function PageHeader({
  leading,
  title,
  subtitle,
  children,
  glow,
}: {
  leading?: ReactNode
  title: ReactNode
  subtitle?: ReactNode
  children?: ReactNode
  glow?: boolean
}) {
  return (
    <header
      className={`@container flex min-h-[62px] shrink-0 items-center gap-2.5 border-b border-line px-6 py-3 ${
        glow ? 'bg-masthead-glow' : ''
      }`}
    >
      <div className="mr-auto flex min-w-0 flex-1 items-center gap-3.5">
        {leading}
        <div className="flex min-w-0 flex-col">
          <h1 className="flex min-w-0 items-center gap-2 text-[17px] font-semibold tracking-tight text-fg">
            {title}
          </h1>
          {subtitle && <div className="mt-0.5 truncate text-xs text-muted">{subtitle}</div>}
        </div>
      </div>
      {children}
    </header>
  )
}
