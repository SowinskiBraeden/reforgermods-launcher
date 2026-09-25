//! Background updates, Windows only.
//!
//! The launcher is an API client, which is what makes this worth shipping in the
//! first release rather than later: when `/v2/servers` changes shape, anyone on
//! an old build gets a broken server browser and blames the site. An updater
//! turns that from a support problem into a non-event.
//!
//! The shape is deliberate, and it is the reason this is not simply
//! `download_and_install`:
//!
//! 1. check and download in the background, so nothing blocks startup;
//! 2. hold the downloaded package until the session ends;
//! 3. run the installer as the app exits, without relaunching it.
//!
//! Nobody is interrupted mid-session, and nobody is asked to make a decision
//! about a patch release. The update is simply there the next time they open the
//! launcher. `Update::install` calls `std::process::exit(0)` on Windows after
//! launching the installer, which is harmless at exit and would be a hard kill at
//! any other moment — another reason the install is pinned to exactly this point.
//!
//! There is no updater on Linux. The Arch package is owned by pacman, and an app
//! that rewrites its own files under `/usr/bin` fights the package manager (see
//! `docs/distribution.md` section 7). The dependency is scoped to Windows in
//! `Cargo.toml`, so this module does not exist in a Linux build.

use std::sync::Mutex;

use tauri::Manager as _;
use tauri_plugin_updater::{Update, UpdaterExt as _};

/// An update that has been downloaded and is waiting for the session to end.
///
/// `Update` carries the verified release metadata and `bytes` is the installer
/// payload, already signature-checked by the plugin against the public key in
/// `tauri.conf.json`. Holding both means the install at exit is a local
/// operation: no network, nothing that can hang a closing app.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Staged>>);

struct Staged {
    update: Update,
    bytes: Vec<u8>,
}

/// Reports a failure only to a developer. An updater that cannot reach the
/// network is not the user's problem and must not produce a dialog; `RFM_DEBUG=1`
/// is how it becomes visible, matching how the rest of the app gates detail.
fn note(context: &str, error: &dyn std::fmt::Display) {
    if crate::debug_mode() {
        eprintln!("updater: {context}: {error}");
    }
}

/// Checks for an update and downloads it, in the background.
///
/// Silent on every failure by design: no endpoint, no network, a malformed
/// manifest or a bad signature all leave the app exactly as it was.
pub fn spawn_check(app: tauri::AppHandle) {
    // A dev build's version always matches the manifest, so the check can only
    // cost a request. Skipped rather than relied upon to no-op.
    if cfg!(debug_assertions) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let updater = match app.updater() {
            Ok(updater) => updater,
            Err(error) => return note("could not build the updater", &error),
        };
        let update = match updater.check().await {
            Ok(Some(update)) => update,
            // Already current: the overwhelmingly common case.
            Ok(None) => return,
            Err(error) => return note("check failed", &error),
        };
        let bytes = match update.download(|_, _| {}, || {}).await {
            Ok(bytes) => bytes,
            Err(error) => return note("download failed", &error),
        };
        if let Some(state) = app.try_state::<PendingUpdate>() {
            *state.0.lock().unwrap() = Some(Staged { update, bytes });
        }
    });
}

/// Runs a staged installer, if there is one. Called from the exit event.
///
/// `restart_after_install(false)` is the whole point: the default relaunches the
/// app, and relaunching something the user has just closed is worse than the
/// stale build it fixes.
pub fn install_staged(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<PendingUpdate>() else {
        return;
    };
    // Taken, not borrowed: if the installer somehow returns, a second attempt
    // during the same shutdown is not wanted.
    let Some(staged) = state.0.lock().unwrap().take() else {
        return;
    };
    if let Err(error) = staged
        .update
        .restart_after_install(false)
        .install(staged.bytes)
    {
        note("install failed", &error);
    }
}
