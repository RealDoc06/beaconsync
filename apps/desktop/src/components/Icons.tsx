// Crisp 16x16 inline SVG icons. Zero runtime dependencies, inherits `currentColor`.

import type { SVGProps } from 'react'

type Props = SVGProps<SVGSVGElement>

function Svg({ className = 'size-4', children, ...props }: Props) {
  return (
    <svg
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={`shrink-0 ${className}`}
      {...props}
    >
      {children}
    </svg>
  )
}

export function LibraryIcon(props: Props) {
  return (
    <Svg {...props}>
      <rect x="2" y="2.5" width="5" height="5" rx="1.2" />
      <rect x="9" y="2.5" width="5" height="5" rx="1.2" />
      <rect x="2" y="9.5" width="5" height="4.5" rx="1.2" />
      <rect x="9" y="9.5" width="5" height="4.5" rx="1.2" />
    </Svg>
  )
}

export function LinkCodeIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M6.5 9.5l3-3" />
      <path d="M5 11a2.5 2.5 0 0 1 0-3.5l1.2-1.2a2.5 2.5 0 0 1 3.5 0" />
      <path d="M11 5a2.5 2.5 0 0 1 0 3.5l-1.2 1.2a2.5 2.5 0 0 1-3.5 0" />
    </Svg>
  )
}

export function SettingsIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M2.5 4.2h4M9.5 4.2h4M2.5 8h7M12.5 8h1M2.5 11.8h2M7.5 11.8h6" />
      <circle cx="8" cy="4.2" r="1.5" />
      <circle cx="11" cy="8" r="1.5" />
      <circle cx="6" cy="11.8" r="1.5" />
    </Svg>
  )
}

export function PlusIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M8 3.2v9.6M3.2 8h9.6" />
    </Svg>
  )
}

export function DownloadIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M8 2.5v7.5M5 7.2 8 10.2l3-3M2.8 13h10.4" />
    </Svg>
  )
}

export function UploadIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M8 10.5V3M5 5.8 8 2.8l3 3M2.8 13h10.4" />
    </Svg>
  )
}

export function PlayIcon(props: Props) {
  return (
    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true" className={`shrink-0 ${props.className ?? 'size-3.5'}`}>
      <path d="M4.2 3.1a.8.8 0 0 1 1.2-.68l7.6 4.9a.8.8 0 0 1 0 1.36l-7.6 4.9a.8.8 0 0 1-1.2-.68V3.1Z" />
    </svg>
  )
}

export function StopIcon(props: Props) {
  return (
    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true" className={`shrink-0 ${props.className ?? 'size-3.5'}`}>
      <rect x="3.5" y="3.5" width="9" height="9" rx="1.6" />
    </svg>
  )
}

export function FolderIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h2.8l1.3 1.5h4.9A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5v-7Z" />
    </Svg>
  )
}

export function FileArchiveIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M4 2.2h5.2L12.8 5.8v8a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V3.2a1 1 0 0 1 1-1Z" />
      <path d="M9 2.2v3.6h3.8" />
      <path d="M6.5 8.5h3M6.5 11h3" />
    </Svg>
  )
}

export function SearchIcon(props: Props) {
  return (
    <Svg {...props}>
      <circle cx="7.2" cy="7.2" r="4.4" />
      <path d="m10.5 10.5 3 3" />
    </Svg>
  )
}

export function PencilIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M10.8 2.6a1.4 1.4 0 0 1 2 2L5.6 11.8l-3 .8.8-3 7.4-7Z" />
    </Svg>
  )
}

export function TrashIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M2.8 4.2h10.4M6 4.2V3a.8.8 0 0 1 .8-.8h2.4a.8.8 0 0 1 .8.8v1.2M4.4 4.2l.6 8.6a1 1 0 0 0 1 .9h4a1 1 0 0 0 1-.9l.6-8.6" />
    </Svg>
  )
}

export function RefreshIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M13 8a5 5 0 1 1-1.3-3.4" />
      <path d="M13 2.5v2.8h-2.8" />
    </Svg>
  )
}

export function GitBranchIcon(props: Props) {
  return (
    <Svg {...props}>
      <circle cx="4.5" cy="3.8" r="1.5" />
      <circle cx="4.5" cy="12.2" r="1.5" />
      <circle cx="11.5" cy="5.8" r="1.5" />
      <path d="M4.5 5.3v5.4M11.5 7.3c0 2.5-2.2 3.4-7 3.4" />
    </Svg>
  )
}

export function GitMergeIcon(props: Props) {
  return (
    <Svg {...props}>
      <circle cx="4.5" cy="3.8" r="1.5" />
      <circle cx="4.5" cy="12.2" r="1.5" />
      <circle cx="11.5" cy="10.2" r="1.5" />
      <path d="M4.5 5.3v5.4M4.5 5.3c0 2.8 2.2 4.9 5.5 4.9" />
    </Svg>
  )
}

export function HistoryIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M2.8 8a5.2 5.2 0 1 0 1.4-3.6" />
      <path d="M2.5 2.5v2.6h2.6" />
      <path d="M8 5.2V8l2 1.2" />
    </Svg>
  )
}

export function DiffIcon(props: Props) {
  return (
    <Svg {...props}>
      <rect x="2.5" y="2.5" width="11" height="11" rx="2" />
      <path d="M8 5.2v3.6M6.2 7h3.6M6.2 10.8h3.6" />
    </Svg>
  )
}

export function CubeIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M8 2 13.2 5v6L8 14l-5.2-3V5L8 2Z" />
      <path d="M2.8 5 8 8l5.2-3M8 8v6" />
    </Svg>
  )
}

export function ServerIcon(props: Props) {
  return (
    <Svg {...props}>
      <rect x="2.2" y="2.5" width="11.6" height="4.4" rx="1.2" />
      <rect x="2.2" y="9.1" width="11.6" height="4.4" rx="1.2" />
      <path d="M4.8 4.7h.01M4.8 11.3h.01M9.5 4.7h2M9.5 11.3h2" />
    </Svg>
  )
}

export function ShareIcon(props: Props) {
  return (
    <Svg {...props}>
      <circle cx="11.8" cy="3.6" r="1.6" />
      <circle cx="4.2" cy="8" r="1.6" />
      <circle cx="11.8" cy="12.4" r="1.6" />
      <path d="m5.6 7.2 4.8-2.8M5.6 8.8l4.8 2.8" />
    </Svg>
  )
}

export function CopyIcon(props: Props) {
  return (
    <Svg {...props}>
      <rect x="5.5" y="5.5" width="8" height="8" rx="1.4" />
      <path d="M3.8 10.5H3.5a1.2 1.2 0 0 1-1.2-1.2V3.5a1.2 1.2 0 0 1 1.2-1.2h5.8a1.2 1.2 0 0 1 1.2 1.2v.3" />
    </Svg>
  )
}

export function CheckIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M3.2 8.4 6.5 11.5 12.8 4.8" />
    </Svg>
  )
}

export function CloseIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="m4 4 8 8M12 4l-8 8" />
    </Svg>
  )
}

export function ChevronRightIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="m6 3.5 4.5 4.5L6 12.5" />
    </Svg>
  )
}

export function ChevronLeftIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M10 3.5 5.5 8 10 12.5" />
    </Svg>
  )
}

export function TerminalIcon(props: Props) {
  return (
    <Svg {...props}>
      <rect x="2" y="2.8" width="12" height="10.4" rx="1.6" />
      <path d="m4.8 6.2 2 1.8-2 1.8M8.4 10h2.8" />
    </Svg>
  )
}

export function ArchiveIcon(props: Props) {
  return (
    <Svg {...props}>
      <rect x="2" y="2.8" width="12" height="3" rx="0.9" />
      <path d="M3.2 5.8v6.4a1.2 1.2 0 0 0 1.2 1.2h7.2a1.2 1.2 0 0 0 1.2-1.2V5.8M6.4 8.8h3.2" />
    </Svg>
  )
}

export function SparklesIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M8 2.2 9.2 5.8 12.8 7 9.2 8.2 8 11.8 6.8 8.2 3.2 7l3.6-1.2L8 2.2Z" />
      <path d="M12.5 10.8l.5 1.4 1.4.5-1.4.5-.5 1.4-.5-1.4-1.4-.5 1.4-.5.5-1.4Z" />
    </Svg>
  )
}

export function SunIcon(props: Props) {
  return (
    <Svg {...props}>
      <circle cx="8" cy="8" r="2.8" />
      <path d="M8 1.8v1.4M8 12.8v1.4M1.8 8h1.4M12.8 8h1.4M3.6 3.6l1 1M11.4 11.4l1 1M12.4 3.6l-1 1M4.6 11.4l-1 1" />
    </Svg>
  )
}

export function MoonIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M13.2 9.8A5.4 5.4 0 0 1 6.2 2.8a5.4 5.4 0 1 0 7 7Z" />
    </Svg>
  )
}

export function MonitorIcon(props: Props) {
  return (
    <Svg {...props}>
      <rect x="2" y="2.6" width="12" height="8" rx="1.4" />
      <path d="M5.6 13.4h4.8M8 10.6v2.8" />
    </Svg>
  )
}

export function UserIcon(props: Props) {
  return (
    <Svg {...props}>
      <circle cx="8" cy="5.4" r="2.6" />
      <path d="M3 13.4a5 5 0 0 1 10 0" />
    </Svg>
  )
}

export function KeyIcon(props: Props) {
  return (
    <Svg {...props}>
      <circle cx="5.4" cy="8" r="2.8" />
      <path d="M8.2 8h5.6M11.5 8v2.2M13.5 8v1.6" />
    </Svg>
  )
}

export function AlertIcon(props: Props) {
  return (
    <Svg {...props}>
      <circle cx="8" cy="8" r="5.8" />
      <path d="M8 5.2v3.3M8 10.8h.01" />
    </Svg>
  )
}

export function UndoIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M3.5 6.2h6.2a3.3 3.3 0 1 1 0 6.6H7.2" />
      <path d="M5.8 3.6 3.2 6.2l2.6 2.6" />
    </Svg>
  )
}

export function EyeOffIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M2.5 2.5 13.5 13.5" />
      <path d="M6.4 4.3A5.8 5.8 0 0 1 14 8a6.8 6.8 0 0 1-1.6 2.1M9.6 11.7A5.8 5.8 0 0 1 2 8a6.8 6.8 0 0 1 2.2-2.6" />
    </Svg>
  )
}

export function GridViewIcon(props: Props) {
  return (
    <Svg {...props}>
      <rect x="2.2" y="2.2" width="4.6" height="4.6" rx="1" />
      <rect x="9.2" y="2.2" width="4.6" height="4.6" rx="1" />
      <rect x="2.2" y="9.2" width="4.6" height="4.6" rx="1" />
      <rect x="9.2" y="9.2" width="4.6" height="4.6" rx="1" />
    </Svg>
  )
}

export function ListViewIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M2.5 4h11M2.5 8h11M2.5 12h11" />
    </Svg>
  )
}

export function ExternalLinkIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M6.5 3.5H3.8a1.2 1.2 0 0 0-1.2 1.2v7.5a1.2 1.2 0 0 0 1.2 1.2h7.5a1.2 1.2 0 0 0 1.2-1.2V9.5" />
      <path d="M9.5 2.5h4v4M13.3 2.7 7.5 8.5" />
    </Svg>
  )
}

export function ShieldCheckIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M8 2 13 4v3.8c0 3.2-2.1 5.3-5 6.2-2.9-.9-5-3-5-6.2V4l5-2Z" />
      <path d="m5.8 8 1.6 1.6 3-3.2" />
    </Svg>
  )
}

export function RocketIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M9.5 6.5 5 11M13 3c-2.6 0-5 1.2-6.6 3.2L4.2 8.4l3.4 3.4 2.2-2.2C11.8 8 13 5.6 13 3Z" />
      <path d="M4.2 8.4 2.5 9.2l1.4 2.9 2.9 1.4.8-1.7" />
    </Svg>
  )
}

export function CommandIcon(props: Props) {
  return (
    <Svg {...props}>
      <path d="M5.5 5.5H4a1.5 1.5 0 1 1 1.5-1.5v8A1.5 1.5 0 1 1 4 10.5h8a1.5 1.5 0 1 1-1.5 1.5V4A1.5 1.5 0 1 1 12 5.5H5.5Z" />
    </Svg>
  )
}
