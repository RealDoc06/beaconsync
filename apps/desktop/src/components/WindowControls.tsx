import { getCurrentWindow } from '@tauri-apps/api/window'
import { useEffect, useState } from 'react'

/**
 * Windows caption buttons for the undecorated main window (`tauri.windows.conf.json` sets
 * `decorations: false`). Glyphs come from the system icon font, so they match native apps;
 * sizes and hover colors follow the Windows 11 caption spec.
 */
export function WindowControls() {
  const [maximized, setMaximized] = useState(false)
  const [focused, setFocused] = useState(true)

  useEffect(() => {
    const win = getCurrentWindow()
    const syncMaximized = () => void win.isMaximized().then(setMaximized)
    syncMaximized()
    const unlisten = [
      win.onResized(syncMaximized),
      win.onFocusChanged(({ payload }) => setFocused(payload)),
    ]
    return () => unlisten.forEach((p) => void p.then((off) => off()))
  }, [])

  const win = getCurrentWindow()
  return (
    <div className={`flex self-stretch ${focused ? 'text-fg' : 'text-subtle'}`}>
      <Caption label="Minimize" glyph={''} onClick={() => void win.minimize()} />
      <Caption
        label={maximized ? 'Restore' : 'Maximize'}
        glyph={maximized ? '' : ''}
        onClick={() => void win.toggleMaximize()}
      />
      <Caption label="Close" glyph={''} onClick={() => void win.close()} danger />
    </div>
  )
}

function Caption({
  label,
  glyph,
  onClick,
  danger,
}: {
  label: string
  glyph: string
  onClick: () => void
  danger?: boolean
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      tabIndex={-1}
      onClick={onClick}
      className={`grid w-[46px] place-items-center font-['Segoe_Fluent_Icons','Segoe_MDL2_Assets'] text-[10px] transition-colors duration-100 ${
        danger ? 'hover:bg-[#c42b1c] hover:text-white active:bg-[#c42b1c]/90' : 'hover:bg-fg/8 active:bg-fg/5'
      }`}
    >
      {glyph}
    </button>
  )
}
