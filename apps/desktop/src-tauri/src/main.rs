//! BeaconSync desktop: a thin Tauri layer over `beacon-core`.
//! Commands live in `library` (instances, imports, joins), `pack` (one pack's files,
//! changes and history) and `hub` (settings, account, publish, deploy).
//! The TypeScript mirror of every command is `src/lib/api.ts`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hub;
mod library;
mod pack;
mod util;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            library::list_instances,
            library::instance_badges,
            library::create_pack,
            library::import_file,
            library::adopt_instance,
            library::install_modrinth_pack,
            library::install_curseforge_pack,
            library::search_modrinth,
            library::search_curseforge_packs,
            library::minecraft_versions,
            library::loader_versions,
            library::preview_code,
            library::join_pack,
            pack::pack_info,
            pack::pack_files,
            pack::set_enabled,
            pack::set_side,
            pack::remove_files,
            pack::add_project,
            pack::check_updates,
            pack::apply_updates,
            pack::status,
            pack::discard,
            pack::save_version,
            pack::abort_merge,
            pack::history,
            pack::restore_version,
            pack::reset_version,
            pack::switch_branch,
            pack::create_branch,
            pack::delete_branch,
            pack::merge_branch,
            pack::resolve_conflicts,
            pack::check_upstream,
            pack::pull_upstream,
            pack::check_release,
            pack::update_subscribed,
            pack::launch,
            pack::rename_pack,
            pack::delete_pack,
            pack::open_external,
            pack::export_mrpack,
            hub::get_settings,
            hub::save_settings,
            hub::sign_in,
            hub::sign_out,
            hub::approve_device,
            hub::publish,
            hub::releases,
            hub::servers,
            hub::deploy,
            hub::export_server_pack,
        ])
        .run(tauri::generate_context!())
        .expect("error while running BeaconSync");
}
