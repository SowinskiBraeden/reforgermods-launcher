//! reforgermods.net launcher: the Tauri application shell.
//!
//! This crate is intentionally thin. It owns the window, the command surface and
//! the wiring between them; all logic lives in `rfm-core`, which has no Tauri
//! dependency and is tested directly.

mod commands;
mod presence;
mod state;
#[cfg(target_os = "windows")]
mod update;

use rfm_core::store::Store;
use state::AppState;
use tauri::Manager as _;

/// True when `RFM_DEBUG` is set to something truthy.
///
/// Gates developer-facing detail; nothing about product behaviour depends on it.
pub fn debug_mode() -> bool {
    std::env::var("RFM_DEBUG")
        .map(|v| matches!(v.trim(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// File the launcher persists settings, favorites, recents and history to.
const STATE_FILE: &str = "launcher-state.json";

/// Builds and runs the application.
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        // Used from Rust only, to hand steam:// and https:// URIs to the OS.
        // The UI is not granted permission to call it.
        .plugin(tauri_plugin_opener::init());

    // Windows only; on Linux the distro package owns updates. See update.rs.
    #[cfg(target_os = "windows")]
    {
        builder = builder
            .plugin(tauri_plugin_updater::Builder::new().build())
            .manage(update::PendingUpdate::default());
    }

    let app = builder
        .setup(|app| {
            let store = Store::load(state_file_path(app.handle()));
            let state = AppState::new(store).map_err(|e| {
                // A client that cannot be built even against the production
                // default means no TLS backend; there is nothing to recover to.
                format!("could not initialise the API client: {e}")
            })?;
            app.manage(state);

            // Background: never blocks the window appearing, and the download is
            // applied when the session ends rather than during it.
            #[cfg(target_os = "windows")]
            update::spawn_check(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_servers,
            commands::get_server,
            commands::get_servers,
            commands::get_server_history,
            commands::get_mod_readiness,
            commands::ping_server,
            commands::ping_regions,
            commands::get_fleet_now,
            commands::list_ping_sites,
            commands::get_status,
            commands::join_server,
            commands::launch_game,
            commands::open_server_page,
            commands::open_discord_invite,
            commands::get_local_state,
            commands::toggle_favorite,
            commands::update_settings,
            commands::get_install_identity,
            commands::is_debug_mode,
            commands::clear_history,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the reforgermods.net launcher");

    // `run` with a callback rather than `Builder::run`, so the exit event can
    // hand off to a staged installer. The callback runs for every event; only
    // Exit is of interest, and it is the last one.
    app.run(|_handle, event| {
        if let tauri::RunEvent::Exit = event {
            #[cfg(target_os = "windows")]
            update::install_staged(_handle);
        }
    });
}

/// Where the state file lives.
///
/// Uses Tauri's resolved per-app config directory when available, falling back to
/// the working directory so a sandboxed environment without a config dir still
/// starts rather than panicking.
fn state_file_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    match app.path().app_config_dir() {
        Ok(dir) => dir.join(STATE_FILE),
        Err(_) => std::path::PathBuf::from(STATE_FILE),
    }
}
