import type { ReactNode } from 'react'

/** Title row at the top of a page with actions on the right. Its empty space drags the window,
 * since on macOS the title bar is drawn over the app. */
export function PageHeader({
  leading,
  title,
  subtitle,
  children,
}: {
  leading?: ReactNode
  title: ReactNode
  subtitle?: ReactNode
  children?: ReactNode
}) {
  return (
    <header
      data-tauri-drag-region
      className="@container flex min-h-[60px] shrink-0 items-center gap-2.5 border-b border-line bg-bg px-6 py-2.5"
    >
      <div className="mr-auto flex min-w-0 flex-1 items-center gap-3">
        {leading}
        <div className="flex min-w-0 flex-col">
          <h1 className="flex min-w-0 items-center gap-2 text-[16px] font-semibold tracking-tight text-fg">
            {title}
          </h1>
          {subtitle && <div className="truncate text-xs text-muted">{subtitle}</div>}
        </div>
      </div>
      {children}
    </header>
  )
}
