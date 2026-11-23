// Theme management: system, light, or dark. Persists in localStorage, updates
// `data-theme` and `color-scheme` on <html>, and syncs the native Tauri window
// chrome when running inside the desktop shell.

import { getCurrentWindow } from '@tauri-apps/api/window'
import { create } from 'zustand'

export type ThemeMode = 'system' | 'light' | 'dark'
export type ResolvedTheme = 'light' | 'dark'

const STORAGE_KEY = 'beaconsync-theme'

// Kept in sync with `--c-bg` in `src/index.css` and `backgroundColor` in `src-tauri/tauri.conf.json`.
const WINDOW_BG: Record<ResolvedTheme, string> = {
  dark: '#0c0d10',
  light: '#f8f7f4',
}

const isBrowser = typeof window !== 'undefined' && typeof document !== 'undefined'
const isLinux = isBrowser && navigator.userAgent.includes('Linux')

function readStoredMode(): ThemeMode {
  if (!isBrowser) return 'dark'
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY)
    if (raw === 'light' || raw === 'dark' || raw === 'system') return raw
  } catch {
    // Storage unavailable; fall back to system.
  }
  return 'system'
}

function systemPreference(): ResolvedTheme {
  if (!isBrowser) return 'dark'
  return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

export function resolveTheme(mode: ThemeMode): ResolvedTheme {
  return mode === 'system' ? systemPreference() : mode
}

function applyDomTheme(resolved: ResolvedTheme) {
  if (!isBrowser) return
  const root = document.documentElement
  root.dataset.theme = resolved
  root.style.colorScheme = resolved
}

function syncWindowBg(resolved: ResolvedTheme) {
  if (!isBrowser || !('__TAURI_INTERNALS__' in window)) return
  try {
    void getCurrentWindow().setBackgroundColor(WINDOW_BG[resolved]).catch(() => {})
  } catch {
    // Ignore outside native Tauri window context.
  }
}

let hasNativeThemeOverride = false

async function syncNativeTheme(mode: ThemeMode, resolved: ResolvedTheme) {
  if (!isBrowser || !('__TAURI_INTERNALS__' in window)) return
  try {
    const win = getCurrentWindow()
    // On Linux, tao 0.37.2's `setTheme(null)` clears GTK's dark preference instead of
    // restoring system follow, so leave GTK's native preference untouched and sync only the
    // webview background color.
    if (isLinux) {
      await win.setBackgroundColor(WINDOW_BG[resolved])
      return
    }
    if (mode !== 'system') {
      hasNativeThemeOverride = true
      await win.setTheme(mode)
      await win.setBackgroundColor(WINDOW_BG[resolved])
      return
    }
    if (hasNativeThemeOverride) {
      hasNativeThemeOverride = false
      await win.setTheme(null)
      const actual = systemPreference()
      if (useTheme.getState().mode === 'system') {
        applyDomTheme(actual)
        useTheme.setState({ resolved: actual })
        await win.setBackgroundColor(WINDOW_BG[actual])
        return
      }
    }
    await win.setBackgroundColor(WINDOW_BG[resolved])
  } catch {
    // Ignore outside native Tauri window context.
  }
}

interface ThemeState {
  mode: ThemeMode
  resolved: ResolvedTheme
  setMode: (mode: ThemeMode) => void
}

const initialMode = readStoredMode()
const initialResolved = resolveTheme(initialMode)

export const useTheme = create<ThemeState>()((set) => ({
  mode: initialMode,
  resolved: initialResolved,
  setMode: (mode) => {
    const resolved = resolveTheme(mode)
    if (isBrowser) {
      try {
        window.localStorage.setItem(STORAGE_KEY, mode)
      } catch {
        // Ignore storage errors.
      }
    }
    applyDomTheme(resolved)
    set({ mode, resolved })
    void syncNativeTheme(mode, resolved)
  },
}))

let initialized = false

/** Call once before React mounts so the first paint uses the active theme. */
export function initTheme() {
  if (!isBrowser || initialized) return
  initialized = true

  const { mode, resolved } = useTheme.getState()
  applyDomTheme(resolved)
  void syncNativeTheme(mode, resolved)

  const mq = window.matchMedia?.('(prefers-color-scheme: dark)')
  if (!mq) return
  const onChange = () => {
    if (useTheme.getState().mode !== 'system') return
    const next = systemPreference()
    applyDomTheme(next)
    syncWindowBg(next)
    useTheme.setState({ resolved: next })
  }
  mq.addEventListener('change', onChange)
}
