// App wide UI state: current view, the running long operation, notices and dialogs.

import { useMutation, useQueryClient, type QueryKey } from '@tanstack/react-query'
import { create } from 'zustand'
import type { Conflict, Progress } from './types'

export type PackTab = 'mods' | 'changes' | 'history' | 'servers' | 'share'

export type View =
  | { name: 'library' }
  | { name: 'import' }
  | { name: 'join'; code?: string }
  | { name: 'settings' }
  | { name: 'pack'; path: string; tab: PackTab }

interface Confirm {
  title: string
  body: string
  action: string
  danger?: boolean
  onConfirm: () => void
}

interface AppState {
  view: View
  go: (view: View) => void
  /** Label of the long operation in progress (one at a time keeps the UI calm). */
  busy: string | null
  progress: Progress | null
  /** `open` is a file the notice offers to open (a crash report). */
  notice: { kind: 'error' | 'ok'; text: string; open?: string } | null
  confirm: Confirm | null
  /** Merge conflicts waiting for the user to pick a side. */
  conflicts: { path: string; items: Conflict[] } | null
  newPackOpen: boolean
  set: (patch: Partial<Omit<AppState, 'set' | 'go'>>) => void
}

export const useApp = create<AppState>()((set) => ({
  view: { name: 'library' },
  go: (view) => set({ view, notice: null }),
  busy: null,
  progress: null,
  notice: null,
  confirm: null,
  conflicts: null,
  newPackOpen: false,
  set,
}))

export const askConfirm = (c: Confirm) => useApp.getState().set({ confirm: c })

interface ActionOptions<V, R> {
  /** Shown in the status bar while running. */
  label: string
  /** Query keys (prefixes) to refetch afterwards. */
  invalidate?: QueryKey[]
  /** Message shown on success. */
  done?: string | ((result: R) => string | null)
  onSuccess?: (result: R, vars: V) => void
}

/**
 * A user triggered operation: shows the label and progress events in the status bar,
 * reports errors as a notice and refreshes the given queries.
 */
export function useAction<V, R>(fn: (vars: V) => Promise<R>, opts: ActionOptions<V, R>) {
  const qc = useQueryClient()
  const set = useApp((s) => s.set)
  return useMutation({
    mutationFn: fn,
    onMutate: () => set({ busy: opts.label, progress: null, notice: null }),
    onSuccess: (result, vars) => {
      const text = typeof opts.done === 'function' ? opts.done(result) : opts.done
      if (text) set({ notice: { kind: 'ok', text } })
      opts.onSuccess?.(result, vars)
    },
    onError: (e) => set({ notice: { kind: 'error', text: e instanceof Error ? e.message : String(e) } }),
    onSettled: async () => {
      set({ busy: null, progress: null })
      await Promise.all((opts.invalidate ?? []).map((queryKey) => qc.invalidateQueries({ queryKey })))
    },
  })
}

/** Query keys for one pack; invalidating `pack(path)` refreshes every tab. */
export const keys = {
  library: ['library'] as const,
  badges: (path: string) => ['badges', path] as const,
  pack: (path: string) => ['pack', path] as const,
  settings: ['settings'] as const,
  accounts: ['accounts'] as const,
  memory: (path: string) => ['memory', path] as const,
  update: ['update'] as const,
}

/** Everything that can change after editing a pack. */
export const packKeys = (path: string): QueryKey[] => [keys.pack(path), keys.badges(path), keys.library]
