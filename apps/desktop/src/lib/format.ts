import type { Loader, LoaderKind } from './types'

export const LOADER_LABEL: Record<LoaderKind, string> = {
  vanilla: 'Vanilla',
  fabric: 'Fabric',
  quilt: 'Quilt',
  forge: 'Forge',
  neoforge: 'NeoForge',
}

export const loaderText = (l: Loader) => (l.version ? `${LOADER_LABEL[l.kind]} ${l.version}` : LOADER_LABEL[l.kind])

export const shortId = (id: string) => id.slice(0, 8)

const units: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 31536000],
  ['month', 2592000],
  ['week', 604800],
  ['day', 86400],
  ['hour', 3600],
  ['minute', 60],
]
const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })

/** "3 hours ago" from unix seconds. */
export function ago(unix: number) {
  const diff = unix - Date.now() / 1000
  for (const [unit, secs] of units) {
    if (Math.abs(diff) >= secs) return rtf.format(Math.round(diff / secs), unit)
  }
  return 'just now'
}

/**
 * Next version name after the newest release (`names` newest first): bumps the last number and
 * keeps what's before it ("v1.3" -> "v1.4"), skipping names already used (after a rollback).
 */
export function nextVersion(names: string[]) {
  const [, prefix, n] = names[0]?.match(/^(.*?)(\d+)\D*$/) ?? []
  if (!n) return 'v1.0'
  let next = Number(n) + 1
  while (names.includes(`${prefix}${next}`)) next++
  return `${prefix}${next}`
}

export function count(n: number) {
  return n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${(n / 1e3).toFixed(0)}k` : String(n)
}
