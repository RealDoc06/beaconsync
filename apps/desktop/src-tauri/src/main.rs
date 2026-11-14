//! BeaconSync desktop: a thin Tauri layer over `beacon-core`.
//! Commands live in `library` (instances, imports, joins), `pack` (one pack's files,
//! changes and history), `hub` (settings, account, publish, deploy) and `updates` (the app
//! updating itself).
//! The TypeScript mirror of every command is `src/lib/api.ts`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(feature = "automation")]
mod automation;
mod hub;
mod library;
mod pack;
mod updates;
mod util;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            updates::start(app.handle());
            #[cfg(feature = "automation")]
            automation::start(app.handle().clone());
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            library::list_instances,
            library::instance_badges,
            library::instance_icon,
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
            pack::ignore_files,
            pack::save_version,
            pack::abort_merge,
            pack::history,
            pack::rollback,
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
            pack::instance_memory,
            pack::set_instance_memory,
            pack::rename_pack,
            pack::delete_pack,
            pack::open_external,
            pack::export_mrpack,
            hub::get_settings,
            hub::save_settings,
            hub::minecraft_accounts,
            hub::set_minecraft_account,
            hub::open_prism,
            hub::add_minecraft_account,
            hub::remove_minecraft_account,
            hub::sign_in,
            hub::sign_out,
            hub::approve_device,
            hub::publish,
            hub::releases,
            hub::servers,
            hub::deploy,
            hub::server_console,
            hub::server_action,
            hub::export_server_pack,
            updates::update_status,
            updates::update_check,
            updates::update_download,
            updates::update_install,
        ])
        .run(tauri::generate_context!())
        .expect("error while running BeaconSync");
}
