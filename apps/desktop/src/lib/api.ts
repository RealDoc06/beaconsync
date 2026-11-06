// Typed mirror of every Tauri command in src-tauri/src. Argument keys are camelCase
// (Tauri converts them to the Rust snake_case parameter names); return types are the
// serde output described in ./types.ts.

import { invoke } from '@tauri-apps/api/core'
import type * as T from './types'

type Cmd<Args, Ret> = { args: Args; ret: Ret }
type None = Record<never, never>
type P<Extra = None> = { path: string } & Extra

export interface Commands {
  // library
  list_instances: Cmd<None, T.LibraryItem[]>
  instance_badges: Cmd<P, T.Badges>
  instance_icon: Cmd<P, string | null>
  create_pack: Cmd<{ name: string; minecraft: string; loader: T.Loader }, string>
  import_file: Cmd<{ file: string }, string>
  adopt_instance: Cmd<{ dir: string }, string>
  install_modrinth_pack: Cmd<{ project: string }, string>
  install_curseforge_pack: Cmd<{ id: number }, string>
  search_modrinth: Cmd<
    { query: string; projectType: T.ProjectType; minecraft: string | null; loader: T.LoaderKind | null; offset: number },
    T.SearchResult
  >
  search_curseforge_packs: Cmd<{ query: string }, T.CfMod[]>
  minecraft_versions: Cmd<None, T.McVersion[]>
  loader_versions: Cmd<{ kind: T.LoaderKind; minecraft: string }, T.LoaderVersion[]>
  preview_code: Cmd<{ input: string }, T.JoinPreview>
  join_pack: Cmd<{ input: string; fork: boolean }, string>
  // pack
  pack_info: Cmd<P, T.PackInfo>
  pack_files: Cmd<P, T.FileRow[]>
  set_enabled: Cmd<P<{ file: string; enabled: boolean }>, string>
  set_side: Cmd<P<{ file: string; side: T.Side }>, null>
  remove_files: Cmd<P<{ files: string[] }>, null>
  add_project: Cmd<P<{ project: string; projectType: T.ProjectType }>, string[]>
  check_updates: Cmd<P, T.ModUpdate[]>
  apply_updates: Cmd<P<{ updates: T.ModUpdate[] }>, null>
  status: Cmd<P, T.Status>
  discard: Cmd<P<{ ids: string[] | null }>, null>
  save_version: Cmd<P<{ message: string }>, string>
  abort_merge: Cmd<P, null>
  history: Cmd<P, T.History>
  rollback: Cmd<P<{ id: string; publish: boolean }>, T.Rollback>
  reset_version: Cmd<P<{ id: string }>, null>
  switch_branch: Cmd<P<{ name: string }>, null>
  create_branch: Cmd<P<{ name: string }>, null>
  delete_branch: Cmd<P<{ name: string }>, null>
  merge_branch: Cmd<P<{ name: string }>, { conflicts: T.Conflict[]; fast_forward: boolean }>
  resolve_conflicts: Cmd<P<{ conflicts: T.Conflict[]; picks: [string, T.Pick][] }>, null>
  check_upstream: Cmd<P, T.UpstreamInfo>
  pull_upstream: Cmd<P, T.Conflict[]>
  check_release: Cmd<P, T.UpdateInfo>
  update_subscribed: Cmd<P, T.UpdateInfo>
  launch: Cmd<P, null>
  rename_pack: Cmd<P<{ name: string }>, null>
  delete_pack: Cmd<P, null>
  open_external: Cmd<{ target: string }, null>
  export_mrpack: Cmd<P<{ out: string }>, null>
  // hub
  get_settings: Cmd<None, T.SettingsView>
  save_settings: Cmd<{ form: T.SettingsForm }, null>
  sign_in: Cmd<{ username: string; password: string; create: boolean }, string>
  sign_out: Cmd<None, null>
  approve_device: Cmd<{ code: string }, null>
  publish: Cmd<P<{ name: string; notes: string }>, T.Release>
  releases: Cmd<P, T.Release[]>
  servers: Cmd<P, T.Server[]>
  deploy: Cmd<P<{ release: string; servers: string[] | null }>, T.DeployResponse>
  server_console: Cmd<P<{ server: string; after: number | null }>, T.ConsoleView>
  server_action: Cmd<P<{ server: string; action: T.ServerAction }>, null>
  export_server_pack: Cmd<P<{ out: string }>, null>
  // updates
  update_status: Cmd<None, T.UpdateView>
  update_download: Cmd<None, null>
  update_install: Cmd<None, null>
}

export type Command = keyof Commands
export type ArgsOf<K extends Command> = Commands[K]['args']
export type RetOf<K extends Command> = Commands[K]['ret']

/** Calls a command. Commands without arguments take none; errors reject with an `Error`. */
export async function call<K extends Command>(
  cmd: K,
  ...args: keyof ArgsOf<K> extends never ? [] : [ArgsOf<K>]
): Promise<RetOf<K>> {
  try {
    return await invoke<RetOf<K>>(cmd, args[0])
  } catch (e) {
    throw new Error(typeof e === 'string' ? e : String(e))
  }
}
