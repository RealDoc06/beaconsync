// Types shared with the Rust side. Each mirrors the serde output of a beacon-core or
// src-tauri struct; field names stay snake_case because serde does not rename them.

export type LoaderKind = 'vanilla' | 'fabric' | 'quilt' | 'forge' | 'neoforge'
export type Side = 'both' | 'client' | 'server'
export type Role = 'local' | 'owner' | 'subscriber'

export interface Loader {
  kind: LoaderKind
  version?: string
}

export type Source =
  | { type: 'modrinth'; project: string; version: string; url: string }
  | { type: 'curseforge'; project: number; file: number; url: string }
  | { type: 'url'; url: string }

/** Tree entry. `side` is omitted when it is `both`. */
export interface Entry {
  hash: string
  size: number
  side?: Side
  name?: string
  version?: string
  source?: Source
}

export interface Remote {
  hub: string
  pack: string
  code: string
  branch: string
}

export type Upstream =
  | { kind: 'beacon'; hub: string; code: string; name: string; commit: string; base_tree: string }
  | { kind: 'modrinth'; project: string; name: string; version: string; base_tree: string }

export interface PackConfig {
  format: number
  name: string
  role: Role
  remote?: Remote
  upstream?: Upstream
}

export interface InstanceInfo {
  id: string
  path: string
  name: string
  minecraft: string
  loader: Loader
  tracked: boolean
}

export interface LibraryItem extends InstanceInfo {
  mod_count: number
  role: Role | null
  upstream: string | null
}

export interface Badges {
  unsaved: number
  update: string | null
}

export interface PackInfo extends InstanceInfo {
  config: PackConfig
  branch: string | null
  share_link: string | null
}

export type ContentFolder = 'mods' | 'resourcepacks' | 'shaderpacks'

export interface FileRow {
  path: string
  folder: ContentFolder
  name: string
  version: string | null
  side: Side
  enabled: boolean
  size: number
  source: Source | null
}

export type ChangeKind = 'added' | 'removed' | 'modified' | 'enabled' | 'disabled'
export type FileRef = Entry & { path: string }

export interface Change {
  id: string
  kind: ChangeKind
  path: string
  old: FileRef | null
  new: FileRef | null
}

export interface TreeDiff {
  minecraft: [string, string] | null
  loader: [Loader, Loader] | null
  files: Change[]
}

export interface Status {
  branch: string | null
  head: string | null
  merging: boolean
  diff: TreeDiff
}

export interface LogEntry {
  id: string
  format: number
  tree: string
  parents: string[]
  author: string
  message: string
  time: number
}

export interface BranchInfo {
  name: string
  commit: string
  current: boolean
}

export interface History {
  head: string | null
  log: LogEntry[]
  branches: BranchInfo[]
}

export type ConflictSide = [path: string, entry: Entry] | null

export interface Conflict {
  id: string
  base: ConflictSide
  ours: ConflictSide
  theirs: ConflictSide
}

export type Pick = 'ours' | 'theirs'

export interface ModUpdate {
  path: string
  current: string | null
  filename: string
  entry: Entry
}

export interface UpdateInfo {
  available: boolean
  name: string
  commit: string | null
  current: string | null
}

export interface UpstreamInfo {
  available: boolean
  name: string
  version: string
}

export interface Release {
  id: string
  commit: string
  name: string
  notes: string
  created: number
}

/** Result of rolling back; `release` is set when it was published and deployed. */
export interface Rollback {
  commit: string
  release: Release | null
  servers: number
}

export interface Server {
  id: string
  name: string
  mode: 'plain' | 'service' | 'docker'
  address: string
  current_commit: string | null
  target_commit: string | null
  target_release: string | null
  status: 'idle' | 'applying' | 'done' | 'failed' | 'offline' | (string & {})
  message: string
  last_seen: number | null
  online: boolean
  /** Version of `beacon` the server runs; null for agents too old to report it (and to update themselves). */
  agent_version: string | null
}

/** Mirrors `api::ServerAction` (serde tag "kind", content "arg"). */
export type ServerAction =
  | { kind: 'command'; arg: string }
  | { kind: 'start' | 'stop' | 'restart' | 'backup' }
  | { kind: 'restore' | 'delete_backup'; arg: string }
  /** Update the server's `beacon` to this BeaconSync version. */
  | { kind: 'update'; arg: string }

export interface Backup {
  /** Path relative to the server folder. */
  id: string
  /** "BeaconSync" or the backup mod, e.g. "FTB Backups". */
  source: string
  note: string
  size: number
  created: number
  /** Whole server, or only the world. */
  full: boolean
}

export interface ConsoleView {
  lines: string[]
  next: number
  backups: Backup[]
  online: boolean
  running: boolean
}

export interface DeployResponse {
  deployment: string
  servers: string[]
}

export interface PublicPack {
  id: string
  code: string
  name: string
  owner: string
  description: string
  minecraft: string
  loader: Loader
  latest: Release | null
  servers: { name: string; address: string; commit: string | null; online: boolean }[]
}

export interface JoinPreview {
  hub: string
  pack: PublicPack
}

export interface McVersion {
  id: string
  type: 'release' | 'snapshot' | 'old_beta' | 'old_alpha' | (string & {})
  releaseTime: string
  url: string
}

export interface LoaderVersion {
  version: string
  stable: boolean
}

export type ProjectType = 'mod' | 'modpack' | 'resourcepack' | 'shader' | 'datapack' | 'plugin'

export interface SearchHit {
  project_id: string
  slug: string
  title: string
  description: string
  icon_url: string | null
  downloads: number
  author: string
  project_type: ProjectType
  client_side: string
  server_side: string
  categories: string[]
}

export interface SearchResult {
  hits: SearchHit[]
  total_hits: number
}

export interface CfMod {
  id: number
  name: string
  summary: string
  logo: { thumbnailUrl: string | null } | null
  downloadCount: number
  authors: { name: string }[]
}

export interface SettingsView {
  hub: string
  hub_custom: boolean
  username: string | null
  signed_in: boolean
  curseforge_key: string
  instances_dir: string
  instances_custom: boolean
  prism_path: string | null
  /** Which Prism plays the game: the player's own, BeaconSync's (downloaded on first play), or none. */
  prism: 'installed' | 'built_in' | 'missing'
}

/** A Minecraft account signed in to Prism (names only, Prism keeps the tokens). */
export interface MinecraftAccount {
  name: string
  offline: boolean
  /** Prism's default account. */
  default: boolean
}

export interface MinecraftAccounts {
  accounts: MinecraftAccount[]
  /** Account the game starts with: picked in BeaconSync, else Prism's default. */
  selected: string | null
  /** Which Prism holds them. Built-in accounts are added and removed from BeaconSync. */
  prism: SettingsView['prism']
}

/** Empty strings reset a field to its default. */
export interface SettingsForm {
  hub: string
  curseforge_key: string
  instances_dir: string
  prism_path: string
}

export interface Progress {
  done: number
  total: number
  label: string
}

/** Mirrors `updates::UpdateView`: the app updating itself. Pushed as the `update` event. */
export interface UpdateView {
  current: string
  /** Newest BeaconSync release, also offered to servers. */
  latest: string | null
  /** Set when `latest` is newer than this app. */
  offer: {
    version: string
    /** Release notes, markdown. */
    notes: string
    /** GitHub release page. */
    url: string
    /** `external`: this install can't update itself, so the button opens `url`. */
    phase: 'available' | 'external' | 'downloading' | 'verifying' | 'ready' | 'installing' | 'failed'
    downloaded: number
    total: number
    error: string | null
  } | null
}
