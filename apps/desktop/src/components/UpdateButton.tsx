// The app's own update, like Musicah's: a small button next to the logo when a newer
// BeaconSync is out. Click downloads (the border fills as progress), click again installs
// and restarts. Hover or focus shows the release notes.

import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { call } from '../lib/api'
import { keys, useApp } from '../lib/store'
import type { UpdateView } from '../lib/types'

type Offer = NonNullable<UpdateView['offer']>

/** The update status; `App` keeps it current from the `update` event. */
export function useUpdate() {
  return useQuery({ queryKey: keys.update, queryFn: () => call('update_status'), staleTime: Infinity }).data
}

/** Rounded square outline starting at top center, so the progress fills clockwise from there. */
const OUTLINE = 'M14 1 H21 A6 6 0 0 1 27 7 V21 A6 6 0 0 1 21 27 H7 A6 6 0 0 1 1 21 V7 A6 6 0 0 1 7 1 Z'

const copy: Record<Offer['phase'], { label: (o: Offer, pct: number) => string; body: string }> = {
  available: { label: (o) => `Download BeaconSync ${o.version}`, body: 'Click to download. You choose when to restart.' },
  external: { label: (o) => `BeaconSync ${o.version} is out`, body: "This install can't update itself. Click to open the release." },
  downloading: { label: (_, pct) => `Downloading ${pct}%`, body: 'Downloading in the background. Keep working.' },
  verifying: { label: () => 'Verifying', body: 'Checking the download against its checksum.' },
  ready: { label: (o) => `Restart to update to ${o.version}`, body: 'Downloaded and verified. Click to install and restart.' },
  installing: { label: () => 'Installing', body: 'Installing. BeaconSync restarts by itself.' },
  failed: { label: () => 'Update failed', body: 'Click to try again.' },
}

export function UpdateButton() {
  const offer = useUpdate()?.offer
  const [open, setOpen] = useState(false)
  const set = useApp((s) => s.set)
  if (!offer) return null
  const pct = offer.total > 0 ? Math.min(100, Math.floor((offer.downloaded / offer.total) * 100)) : 0
  const busy = offer.phase === 'downloading' || offer.phase === 'verifying' || offer.phase === 'installing'
  const filled = offer.phase === 'ready' ? 100 : busy ? pct : 0
  const tone = offer.phase === 'failed' ? 'text-bad' : offer.phase === 'ready' ? 'text-ok' : 'text-accent'
  const text = copy[offer.phase]
  const act = async () => {
    try {
      if (offer.phase === 'external') await call('open_external', { target: offer.url })
      else if (offer.phase === 'ready') await call('update_install')
      else if (!busy) await call('update_download')
    } catch (e) {
      set({ notice: { kind: 'error', text: e instanceof Error ? e.message : String(e) } })
    }
  }

  return (
    <div
      className="relative ml-auto"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
      onFocus={() => setOpen(true)}
      // Stays open while focus moves into the panel (Tab to "Release on GitHub").
      onBlur={(e) => !e.currentTarget.contains(e.relatedTarget) && setOpen(false)}
    >
      <button
        type="button"
        aria-label={text.label(offer, pct)}
        aria-busy={busy}
        onClick={() => void act()}
        className={`relative grid size-7 place-items-center rounded-md hover:bg-hover ${tone}`}
      >
        {(busy || offer.phase === 'ready') && (
          <svg viewBox="0 0 28 28" className="absolute inset-0 size-7" aria-hidden="true">
            <path d={OUTLINE} fill="none" stroke="#262626" strokeWidth="1.5" />
            <path
              d={OUTLINE}
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              pathLength={100}
              strokeDasharray="100"
              strokeDashoffset={100 - filled}
              className="transition-[stroke-dashoffset] duration-300"
            />
          </svg>
        )}
        <Icon phase={offer.phase} />
      </button>
      {open && (
        // Padding instead of a margin, so moving the pointer into the panel keeps it open.
        <div className="absolute top-full left-0 z-50 pt-2">
          <section className="flex w-80 flex-col gap-2 rounded-md border border-line-strong bg-bg p-3 shadow-[0_8px_30px_#000]">
            <strong>{text.label(offer, pct)}</strong>
            <p className="text-muted">{offer.error && offer.phase !== 'available' ? offer.error : text.body}</p>
            <Notes markdown={offer.notes} />
            <button
              type="button"
              className="self-start text-xs text-accent hover:underline"
              onClick={() => void call('open_external', { target: offer.url })}
            >
              Release on GitHub ›
            </button>
          </section>
        </div>
      )}
    </div>
  )
}

function Icon({ phase }: { phase: Offer['phase'] }) {
  const d =
    phase === 'ready' || phase === 'installing'
      ? 'M12.5 4.5A5.5 5.5 0 1 0 13.5 9M12.5 1.5v3h-3' // restart
      : phase === 'failed'
        ? 'M8 4v5M8 11.5v.5' // !
        : 'M8 2.5v8M4.5 7.5 8 11l3.5-3.5M3.5 13.5h9' // download
  return (
    <svg viewBox="0 0 16 16" className="size-4" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
      <path d={d} />
    </svg>
  )
}

/** Release notes as plain lines: headings (`#` or a whole `**line**`) bold, `- ` items as a list. */
function Notes({ markdown }: { markdown: string }) {
  const lines = markdown
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean)
    .map((l) => ({ heading: /^#|^\*\*[^*]+\*\*$/.test(l), text: l.replace(/^#+\s*|\*\*|`/g, '') }))
  if (!lines.length) return null
  return (
    <div className="flex max-h-72 flex-col gap-1 overflow-auto border-t border-line pt-2 text-xs leading-5 select-text">
      {lines.map((l, i) =>
        l.heading ? (
          <div key={i} className="mt-1 font-semibold first:mt-0">
            {l.text}
          </div>
        ) : (
          <div key={i} className="text-[#cfcfcf]">
            {l.text.startsWith('- ') ? `· ${l.text.slice(2)}` : l.text}
          </div>
        ),
      )}
    </div>
  )
}
