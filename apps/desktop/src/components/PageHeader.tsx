import type { ReactNode } from 'react'

/** Title row at the top of a page with actions on the right. Its empty space drags the window,
 * since on macOS the title bar is drawn over the app. */
export function PageHeader({ title, children }: { title: ReactNode; children?: ReactNode }) {
  return (
    <header data-tauri-drag-region className="flex h-14 shrink-0 items-center gap-2 border-b border-line px-5">
      <h1 className="mr-auto min-w-0 truncate text-[15px] font-semibold">{title}</h1>
      {children}
    </header>
  )
}
