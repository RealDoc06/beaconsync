import { getCurrentWindow } from '@tauri-apps/api/window'
import { useEffect, useState } from 'react'

/**
 * Windows caption buttons for the undecorated main window (`tauri.windows.conf.json` sets
 * `decorations: false`). Glyphs come from the system icon font, so they match native apps;
 * sizes and hover colors follow the Windows 11 caption spec. Like native caption buttons they
 * stay out of the Tab order (keyboard users have Alt+F4 and Win+arrows), and they sit above
 * dialog backdrops (`z-50`) so the window can still be minimized or closed with a dialog open.
 */
export function WindowControls() {
  const [maximized, setMaximized] = useState(false)
  const [focused, setFocused] = useState(true)

  useEffect(() => {
    const win = getCurrentWindow()
    const syncMaximized = () => void win.isMaximized().then(setMaximized)
    const unlisten = [
      win.onResized(syncMaximized),
      win.onFocusChanged(({ payload }) => setFocused(payload)),
    ]
    syncMaximized()
    void win.isFocused().then(setFocused)
    return () => unlisten.forEach((p) => void p.then((off) => off()))
  }, [])

  const win = getCurrentWindow()
  return (
    <div className={`relative z-[60] flex self-stretch ${focused ? 'text-fg' : 'text-subtle'}`}>
      <Caption label="Minimize" glyph={'\uE921'} onClick={() => void win.minimize()} />
      <Caption
        label={maximized ? 'Restore' : 'Maximize'}
        glyph={maximized ? '\uE923' : '\uE922'}
        onClick={() => void win.toggleMaximize()}
      />
      <Caption label="Close" glyph={'\uE8BB'} onClick={() => void win.close()} danger />
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
