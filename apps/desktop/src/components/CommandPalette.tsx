// Global Cmd+K / Ctrl+K command palette for instant navigation, pack jumping,
// actions, and theme switching.

import { useQuery } from '@tanstack/react-query'
import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react'
import { call } from '../lib/api'
import { keys, useApp } from '../lib/store'
import { useTheme } from '../lib/theme'
import {
  CubeIcon,
  DownloadIcon,
  LibraryIcon,
  LinkCodeIcon,
  MonitorIcon,
  MoonIcon,
  PlusIcon,
  SearchIcon,
  SettingsIcon,
  SunIcon,
} from './Icons'
import { Dialog } from './ui'

interface CommandItem {
  id: string
  group: string
  label: string
  hint?: string
  icon: ReactNode
  run: () => void
}

export function CommandPalette({ open, onClose }: { open: boolean; onClose: () => void }) {
  if (!open) return null
  return <PaletteDialog onClose={onClose} />
}

function PaletteDialog({ onClose }: { onClose: () => void }) {
  const go = useApp((s) => s.go)
  const setApp = useApp((s) => s.set)
  const setTheme = useTheme((s) => s.setMode)
  const library = useQuery({ queryKey: keys.library, queryFn: () => call('list_instances') })
  const [query, setQuery] = useState('')
  const [activeIdx, setActiveIdx] = useState(0)
  const listId = useId()
  const listRef = useRef<HTMLDivElement>(null)

  const items = useMemo<CommandItem[]>(() => {
    const tracked = (library.data ?? []).filter((i) => i.tracked)
    const list: CommandItem[] = [
      ...tracked.map((p) => ({
        id: `pack-${p.path}`,
        group: 'Packs',
        label: p.name,
        hint: `Minecraft ${p.minecraft}`,
        icon: <CubeIcon className="size-4 text-accent-text" />,
        run: () => go({ name: 'pack', path: p.path, tab: 'mods' }),
      })),
      {
        id: 'act-new',
        group: 'Actions',
        label: 'New pack',
        hint: 'Create a tracked modpack',
        icon: <PlusIcon className="size-4 text-accent-text" />,
        run: () => setApp({ newPackOpen: true }),
      },
      {
        id: 'nav-library',
        group: 'Navigation',
        label: 'Go to Library',
        icon: <LibraryIcon className="size-4" />,
        run: () => go({ name: 'library' }),
      },
      {
        id: 'nav-import',
        group: 'Navigation',
        label: 'Import modpack',
        hint: '.mrpack, CurseForge, Prism',
        icon: <DownloadIcon className="size-4" />,
        run: () => go({ name: 'import' }),
      },
      {
        id: 'nav-join',
        group: 'Navigation',
        label: 'Join with code',
        icon: <LinkCodeIcon className="size-4" />,
        run: () => go({ name: 'join' }),
      },
      {
        id: 'nav-settings',
        group: 'Navigation',
        label: 'Open Settings',
        icon: <SettingsIcon className="size-4" />,
        run: () => go({ name: 'settings' }),
      },
      {
        id: 'theme-dark',
        group: 'Appearance',
        label: 'Switch to Dark theme',
        icon: <MoonIcon className="size-4" />,
        run: () => setTheme('dark'),
      },
      {
        id: 'theme-light',
        group: 'Appearance',
        label: 'Switch to Light theme',
        icon: <SunIcon className="size-4" />,
        run: () => setTheme('light'),
      },
      {
        id: 'theme-system',
        group: 'Appearance',
        label: 'Use System theme',
        icon: <MonitorIcon className="size-4" />,
        run: () => setTheme('system'),
      },
    ]
    const q = query.trim().toLowerCase()
    if (!q) return list
    return list.filter(
      (i) =>
        i.label.toLowerCase().includes(q) ||
        i.group.toLowerCase().includes(q) ||
        (i.hint?.toLowerCase().includes(q) ?? false),
    )
  }, [library.data, query, go, setApp, setTheme])

  useEffect(() => {
    setActiveIdx(0)
  }, [query])

  const clamped = items.length ? Math.min(activeIdx, items.length - 1) : 0
  const activeItem = items[clamped]

  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>('[aria-selected="true"]')
    el?.scrollIntoView({ block: 'nearest' })
  }, [clamped, items.length])

  const execute = (item: CommandItem | undefined) => {
    if (!item) return
    onClose()
    item.run()
  }

  return (
    <Dialog title="Quick command" onClose={onClose}>
      <div className="-m-5 flex flex-col">
        <div className="flex items-center gap-2.5 border-b border-line px-4 py-3 focus-within:border-accent">
          <SearchIcon className="size-4 shrink-0 text-muted" />
          <input
            role="combobox"
            aria-label="Search commands and packs"
            aria-expanded="true"
            aria-controls={listId}
            aria-autocomplete="list"
            aria-activedescendant={activeItem ? `${listId}-${clamped}` : undefined}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.nativeEvent.isComposing) return
              if (e.key === 'ArrowDown') {
                e.preventDefault()
                setActiveIdx((i) => (items.length ? (i + 1) % items.length : 0))
              } else if (e.key === 'ArrowUp') {
                e.preventDefault()
                setActiveIdx((i) => (items.length ? (i - 1 + items.length) % items.length : 0))
              } else if (e.key === 'Enter') {
                e.preventDefault()
                execute(items[clamped])
              }
            }}
            placeholder="Jump to a pack, view, or theme…"
            className="h-7 min-w-0 flex-1 rounded-md bg-transparent px-1.5 text-[13.5px] text-fg outline-none placeholder:text-subtle"
          />
        </div>
        <div ref={listRef} id={listId} role="listbox" aria-label="Commands" className="max-h-80 overflow-auto p-2">
          {items.length === 0 ? (
            <div className="py-8 text-center text-xs text-muted">No matching commands</div>
          ) : (
            items.map((item, idx) => {
              const active = idx === clamped
              const showGroup = idx === 0 || items[idx - 1]?.group !== item.group
              return (
                <div key={item.id}>
                  {showGroup && (
                    <div className="px-3 pt-2 pb-1 text-[10.5px] font-semibold uppercase tracking-[0.06em] text-subtle first:pt-1">
                      {item.group}
                    </div>
                  )}
                  <button
                    id={`${listId}-${idx}`}
                    type="button"
                    role="option"
                    aria-selected={active}
                    onMouseEnter={() => setActiveIdx(idx)}
                    onClick={() => execute(item)}
                    className={`flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-[13px] transition-colors ${
                      active ? 'bg-active text-fg ring-1 ring-accent' : 'text-fg-secondary hover:bg-hover'
                    }`}
                  >
                    <span className={active ? 'text-accent-text' : 'text-muted'}>{item.icon}</span>
                    <span className="min-w-0 flex-1 truncate font-medium">{item.label}</span>
                    {item.hint && <span className="shrink-0 text-xs text-muted">{item.hint}</span>}
                  </button>
                </div>
              )
            })
          )}
        </div>
      </div>
    </Dialog>
  )
}
