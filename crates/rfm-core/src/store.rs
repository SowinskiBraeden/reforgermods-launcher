//! Local persistence: settings, favorites, recent servers and launch history.
//!
//! One JSON document written atomically. SQLite would be the reflex choice, but
//! this is a few kilobytes of user preference that is read once at startup and
//! rewritten on change — a database engine would add a dependency, a schema and
//! a migration story for no benefit at this size.
//!
//! [`LocalState`] holds all the mutation rules and is pure, so the list-trimming
//! and ordering behaviour is unit-tested without touching a filesystem.
//! [`Store`] is the thin IO wrapper around it.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Most recent servers retained.
pub const MAX_RECENTS: usize = 30;
/// Most launch records retained.
pub const MAX_HISTORY: usize = 200;
/// Bumped when the on-disk shape changes incompatibly.
const STATE_VERSION: u32 = 1;

/// Errors from reading or writing the state file.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not read launcher state at {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not write launcher state to {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not serialise launcher state")]
    Serialise(#[source] serde_json::Error),
}

/// User-visible application preferences.
///
/// The API origin is deliberately **not** here. The launcher is powered by
/// reforgermods.net and points at it unconditionally; a user-editable origin
/// would let a settings file redirect every request, including the install
/// identifier, to an arbitrary host. Development overrides go through the
/// `RFM_API_BASE_URL` environment variable instead — see [`crate::api`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Servers fetched per page. Larger pages mean fewer requests.
    pub per_page: u32,
    /// Refresh the list automatically while the window is visible.
    pub auto_refresh: bool,
    /// Seconds between automatic refreshes. Floored at 15 so the launcher
    /// cannot be configured into polling the API hard.
    pub refresh_interval_secs: u32,
    /// Publish what the launcher is doing to Discord Rich Presence.
    ///
    /// On by default: showing the launcher as the running application is the
    /// point of the integration. It does broadcast the name of the server being
    /// viewed to the user's Discord friends, so it is a visible toggle.
    pub discord_presence: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            per_page: 100,
            auto_refresh: true,
            refresh_interval_secs: 60,
            discord_presence: true,
        }
    }
}

impl Settings {
    /// Minimum automatic refresh interval, in seconds.
    pub const MIN_REFRESH_SECS: u32 = 15;

    /// Clamps values that arrived from disk or the UI into usable ranges.
    pub fn sanitise(&mut self) {
        self.per_page = self.per_page.clamp(10, crate::api::MAX_PER_PAGE);
        self.refresh_interval_secs = self.refresh_interval_secs.max(Self::MIN_REFRESH_SECS);
    }
}

/// A server the user recently looked at or joined.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentServer {
    pub id: String,
    /// Name as it was when last seen, so the entry stays readable even if the
    /// server drops out of the index.
    pub name: String,
    /// Unix seconds.
    pub last_seen_at: u64,
}

/// One join attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRecord {
    pub server_id: String,
    pub server_name: String,
    /// Unix seconds.
    pub launched_at: u64,
    /// False when handing the URI to the OS failed.
    pub succeeded: bool,
}

/// Everything the launcher persists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LocalState {
    pub version: u32,
    /// Random fallback identifier, used only when no machine source is
    /// readable. See [`crate::identity`].
    pub install_id: String,
    pub settings: Settings,
    /// Favorite server ids, in the order the user added them.
    pub favorites: Vec<String>,
    /// Recent servers, most recent first.
    pub recents: Vec<RecentServer>,
    /// Launch history, most recent first.
    pub history: Vec<LaunchRecord>,
}

impl Default for LocalState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            install_id: String::new(),
            settings: Settings::default(),
            favorites: Vec::new(),
            recents: Vec::new(),
            history: Vec::new(),
        }
    }
}

impl LocalState {
    /// Repairs state loaded from disk: clamps settings, drops ids that are not
    /// valid server ids, and de-duplicates. A hand-edited or partially written
    /// file must not be able to put the launcher into a broken state.
    pub fn sanitise(&mut self) {
        self.version = STATE_VERSION;
        // Generated on first run and kept thereafter. A hand-edited or
        // truncated value is replaced rather than sent as-is.
        if !is_install_id(&self.install_id) {
            self.install_id = new_install_id();
        }
        self.settings.sanitise();

        let mut seen = HashSet::new();
        self.favorites
            .retain(|id| is_server_id(id) && seen.insert(id.to_ascii_lowercase()));

        let mut seen = HashSet::new();
        self.recents
            .retain(|r| is_server_id(&r.id) && seen.insert(r.id.to_ascii_lowercase()));
        self.recents
            .sort_by_key(|r| std::cmp::Reverse(r.last_seen_at));
        self.recents.truncate(MAX_RECENTS);

        self.history.retain(|h| is_server_id(&h.server_id));
        self.history
            .sort_by_key(|h| std::cmp::Reverse(h.launched_at));
        self.history.truncate(MAX_HISTORY);
    }

    /// True when `id` is a favorite.
    pub fn is_favorite(&self, id: &str) -> bool {
        self.favorites.iter().any(|f| f.eq_ignore_ascii_case(id))
    }

    /// Adds or removes `id` from favorites. Returns the new state, or `None`
    /// when `id` is not a valid server id.
    pub fn toggle_favorite(&mut self, id: &str) -> Option<bool> {
        if !is_server_id(id) {
            return None;
        }
        if self.is_favorite(id) {
            self.favorites.retain(|f| !f.eq_ignore_ascii_case(id));
            Some(false)
        } else {
            self.favorites.push(id.to_ascii_lowercase());
            Some(true)
        }
    }

    /// Records that the user opened `id`, moving it to the front of recents.
    pub fn touch_recent(&mut self, id: &str, name: &str, now: u64) {
        if !is_server_id(id) {
            return;
        }
        let id = id.to_ascii_lowercase();
        self.recents.retain(|r| r.id != id);
        self.recents.insert(
            0,
            RecentServer {
                id,
                name: name.to_owned(),
                last_seen_at: now,
            },
        );
        self.recents.truncate(MAX_RECENTS);
    }

    /// Appends a launch record, newest first.
    pub fn record_launch(&mut self, id: &str, name: &str, succeeded: bool, now: u64) {
        if !is_server_id(id) {
            return;
        }
        self.history.insert(
            0,
            LaunchRecord {
                server_id: id.to_ascii_lowercase(),
                server_name: name.to_owned(),
                launched_at: now,
                succeeded,
            },
        );
        self.history.truncate(MAX_HISTORY);
    }
}

/// Generates the random fallback identifier.
///
/// Used only where no machine source is readable; normally the identifier is
/// derived by [`crate::identity::stable_install_id`] so it survives a
/// reinstall. See `docs/client-identification.md`.
pub fn new_install_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// True for a value shaped like [`new_install_id`] output.
fn is_install_id(value: &str) -> bool {
    crate::launch::ServerId::parse(value).is_ok()
}

/// Canonical-UUID check, mirroring [`crate::launch::ServerId`].
///
/// Persistence accepts only ids that could actually be launched, so a corrupt
/// favorites list cannot smuggle arbitrary text toward the URI builder.
fn is_server_id(id: &str) -> bool {
    crate::launch::ServerId::parse(id).is_ok()
}

/// Current unix time in seconds. Saturates at 0 if the clock is before 1970.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The on-disk state file.
pub struct Store {
    path: PathBuf,
    state: Mutex<LocalState>,
}

impl Store {
    /// Loads state from `path`, or starts from defaults when the file is absent.
    ///
    /// A file that exists but cannot be parsed is treated as absent rather than
    /// fatal: losing preferences is better than a launcher that will not start.
    /// The unreadable file is kept alongside as `*.corrupt` for inspection.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let mut state = match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<LocalState>(&raw) {
                Ok(state) => state,
                Err(_) => {
                    let _ = std::fs::rename(&path, path.with_extension("corrupt"));
                    LocalState::default()
                }
            },
            Err(_) => LocalState::default(),
        };
        state.sanitise();
        Self {
            path,
            state: Mutex::new(state),
        }
    }

    /// The file this store persists to.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads a value out of the state under lock.
    pub fn read<T>(&self, f: impl FnOnce(&LocalState) -> T) -> T {
        let guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
        f(&guard)
    }

    /// The identifier to send with API requests.
    ///
    /// Machine-derived where possible, so it survives a reinstall; the stored
    /// random value is the fallback.
    pub fn install_id(&self) -> String {
        crate::identity::stable_install_id(&self.read(|state| state.install_id.clone()))
    }

    /// Mutates the state and writes it to disk.
    ///
    /// The value `f` returns is passed through, so callers get the result of
    /// their own mutation without a second lock acquisition.
    pub fn write<T>(&self, f: impl FnOnce(&mut LocalState) -> T) -> Result<T, StoreError> {
        let (out, snapshot) = {
            let mut guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let out = f(&mut guard);
            guard.sanitise();
            (out, guard.clone())
        };
        self.persist(&snapshot)?;
        Ok(out)
    }

    /// Serialises `state` and replaces the file atomically.
    fn persist(&self, state: &LocalState) -> Result<(), StoreError> {
        let json = serde_json::to_string_pretty(state).map_err(StoreError::Serialise)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| StoreError::Write {
                path: self.path.clone(),
                source,
            })?;
        }
        // Write beside the target then rename, so a crash mid-write cannot leave
        // a truncated state file behind.
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|source| StoreError::Write {
            path: tmp.clone(),
            source,
        })?;
        std::fs::rename(&tmp, &self.path).map_err(|source| StoreError::Write {
            path: self.path.clone(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID_A: &str = "73e8fb7f-8789-4d88-8aa8-ea69b9aa092f";
    const ID_B: &str = "25888090-0d2b-4d0a-80b1-9145ed9c76c2";

    fn temp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("rfm-store-test-{}-{name}.json", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn the_api_origin_is_not_a_setting() {
        // Serialised settings must not carry an origin a user could redirect.
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert!(!json.contains("api"), "{json}");
        assert!(!json.contains("http"), "{json}");
    }

    #[test]
    fn the_server_list_refreshes_by_default() {
        let s = Settings::default();
        assert!(s.auto_refresh);
        assert!(s.refresh_interval_secs >= Settings::MIN_REFRESH_SECS);
    }

    #[test]
    fn discord_presence_is_on_by_default_and_toggleable() {
        assert!(Settings::default().discord_presence);
        let s: Settings = serde_json::from_str(r#"{"discordPresence":false}"#).unwrap();
        assert!(!s.discord_presence);
    }

    #[test]
    fn settings_are_clamped() {
        let mut s = Settings {
            per_page: 100_000,
            refresh_interval_secs: 1,
            ..Default::default()
        };
        s.sanitise();
        assert_eq!(s.per_page, crate::api::MAX_PER_PAGE);
        assert_eq!(s.refresh_interval_secs, Settings::MIN_REFRESH_SECS);

        let mut s = Settings {
            per_page: 1,
            ..Default::default()
        };
        s.sanitise();
        assert_eq!(s.per_page, 10);
    }

    #[test]
    fn favorites_toggle_and_are_case_insensitive() {
        let mut state = LocalState::default();
        assert_eq!(state.toggle_favorite(ID_A), Some(true));
        assert!(state.is_favorite(ID_A));
        assert!(state.is_favorite(&ID_A.to_uppercase()));
        assert_eq!(state.toggle_favorite(&ID_A.to_uppercase()), Some(false));
        assert!(!state.is_favorite(ID_A));
        assert!(state.favorites.is_empty());
    }

    #[test]
    fn favorites_reject_invalid_ids() {
        let mut state = LocalState::default();
        assert_eq!(state.toggle_favorite("not-a-uuid"), None);
        assert_eq!(state.toggle_favorite(""), None);
        assert!(state.favorites.is_empty());
    }

    #[test]
    fn favorites_do_not_duplicate() {
        let mut state = LocalState {
            favorites: vec![ID_A.to_string(), ID_A.to_uppercase(), ID_B.to_string()],
            ..Default::default()
        };
        state.sanitise();
        assert_eq!(state.favorites, vec![ID_A.to_string(), ID_B.to_string()]);
    }

    #[test]
    fn recents_move_to_front_without_duplicating() {
        let mut state = LocalState::default();
        state.touch_recent(ID_A, "A", 100);
        state.touch_recent(ID_B, "B", 200);
        state.touch_recent(ID_A, "A renamed", 300);
        assert_eq!(state.recents.len(), 2);
        assert_eq!(state.recents[0].id, ID_A);
        assert_eq!(state.recents[0].name, "A renamed");
        assert_eq!(state.recents[0].last_seen_at, 300);
        assert_eq!(state.recents[1].id, ID_B);
    }

    #[test]
    fn recents_are_bounded() {
        let mut state = LocalState::default();
        for i in 0..(MAX_RECENTS + 20) {
            // Distinct valid UUIDs.
            let id = format!("{:08x}-8789-4d88-8aa8-ea69b9aa092f", i);
            state.touch_recent(&id, "s", i as u64);
        }
        assert_eq!(state.recents.len(), MAX_RECENTS);
        // Newest kept.
        assert!(state.recents[0].last_seen_at > state.recents[MAX_RECENTS - 1].last_seen_at);
    }

    #[test]
    fn history_is_newest_first_and_bounded() {
        let mut state = LocalState::default();
        for i in 0..(MAX_HISTORY + 5) {
            state.record_launch(ID_A, "A", i % 2 == 0, i as u64);
        }
        assert_eq!(state.history.len(), MAX_HISTORY);
        assert!(state.history[0].launched_at > state.history[1].launched_at);
    }

    #[test]
    fn history_and_recents_reject_invalid_ids() {
        let mut state = LocalState::default();
        state.record_launch("../../etc/passwd", "x", true, 1);
        state.touch_recent("steam://run/1874880", "x", 1);
        assert!(state.history.is_empty());
        assert!(state.recents.is_empty());
    }

    #[test]
    fn sanitise_drops_garbage_loaded_from_disk() {
        let mut state = LocalState {
            favorites: vec!["junk".into(), ID_A.into()],
            recents: vec![
                RecentServer {
                    id: "junk".into(),
                    name: "x".into(),
                    last_seen_at: 9,
                },
                RecentServer {
                    id: ID_B.into(),
                    name: "b".into(),
                    last_seen_at: 1,
                },
            ],
            history: vec![LaunchRecord {
                server_id: "junk".into(),
                server_name: "x".into(),
                launched_at: 1,
                succeeded: true,
            }],
            ..Default::default()
        };
        state.sanitise();
        assert_eq!(state.favorites, vec![ID_A.to_string()]);
        assert_eq!(state.recents.len(), 1);
        assert_eq!(state.recents[0].id, ID_B);
        assert!(state.history.is_empty());
        assert_eq!(state.version, STATE_VERSION);
    }

    #[test]
    fn the_sent_id_is_machine_derived_where_possible() {
        let path = temp_path("identity");
        let store = Store::load(&path);
        // Force the state file to exist so the fallback is genuinely persisted.
        store.write(|_| ()).unwrap();

        let sent = store.install_id();
        assert!(is_install_id(&sent), "{sent}");
        assert_eq!(
            Store::load(&path).install_id(),
            sent,
            "unstable across loads"
        );

        // The point of deriving from the machine: discarding local state — an
        // uninstall — must not produce a new identity.
        std::fs::remove_file(&path).unwrap();
        let after_reinstall = Store::load(&path).install_id();
        if crate::identity::is_machine_derived() {
            assert_eq!(
                after_reinstall, sent,
                "identity did not survive a reinstall"
            );
        } else {
            assert_ne!(after_reinstall, sent, "a random fallback should rotate");
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_fallback_id_is_generated_on_first_run() {
        let mut state = LocalState::default();
        assert_eq!(state.install_id, "");
        state.sanitise();
        assert!(is_install_id(&state.install_id), "{}", state.install_id);
    }

    #[test]
    fn the_install_id_is_stable_across_loads() {
        let mut state = LocalState::default();
        state.sanitise();
        let first = state.install_id.clone();
        state.sanitise();
        assert_eq!(state.install_id, first, "must not rotate on every load");
    }

    #[test]
    fn a_tampered_install_id_is_replaced_rather_than_sent() {
        for bad in ["", "  ", "not-a-uuid", "<script>", "a".repeat(400).as_str()] {
            let mut state = LocalState {
                install_id: bad.to_string(),
                ..Default::default()
            };
            state.sanitise();
            assert!(is_install_id(&state.install_id), "{bad:?}");
        }
    }

    #[test]
    fn install_ids_are_unique_per_install() {
        let ids: HashSet<String> = (0..64).map(|_| new_install_id()).collect();
        assert_eq!(ids.len(), 64, "ids must not collide");
    }

    #[test]
    fn state_round_trips_through_json() {
        let mut state = LocalState::default();
        state.toggle_favorite(ID_A);
        state.touch_recent(ID_B, "B", 42);
        state.record_launch(ID_A, "A", true, 43);
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<LocalState>(&json).unwrap(), state);
    }

    #[test]
    fn partial_json_loads_with_defaults() {
        let state: LocalState = serde_json::from_str(r#"{"favorites":[]}"#).unwrap();
        assert_eq!(state.settings, Settings::default());
        assert!(state.recents.is_empty());
    }

    #[test]
    fn a_state_file_from_the_previous_version_still_loads() {
        // Older files carry settings.apiBaseUrl and no installId. The origin is
        // ignored, and an id is generated.
        let raw = r#"{"version":1,"settings":{"apiBaseUrl":"https://evil.example","perPage":50},
                      "favorites":[],"recents":[],"history":[]}"#;
        let mut state: LocalState = serde_json::from_str(raw).unwrap();
        state.sanitise();
        assert_eq!(state.settings.per_page, 50, "known settings are kept");
        assert!(is_install_id(&state.install_id));
        let json = serde_json::to_string(&state).unwrap();
        assert!(!json.contains("evil.example"), "origin must not round-trip");
    }

    #[test]
    fn favorites_survive_a_save_and_reload() {
        let path = temp_path("roundtrip");
        {
            let store = Store::load(&path);
            store.write(|s| s.toggle_favorite(ID_A)).unwrap();
            store.write(|s| s.touch_recent(ID_B, "B", 1234)).unwrap();
        }
        let reopened = Store::load(&path);
        assert!(reopened.read(|s| s.is_favorite(ID_A)));
        assert_eq!(reopened.read(|s| s.recents[0].name.clone()), "B");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn missing_file_yields_defaults() {
        let path = temp_path("missing");
        let store = Store::load(&path);
        assert!(store.read(|s| s.favorites.is_empty()));
        assert_eq!(store.read(|s| s.settings.clone()), Settings::default());
    }

    #[test]
    fn corrupt_file_is_set_aside_and_does_not_block_startup() {
        let path = temp_path("corrupt");
        std::fs::write(&path, "{ this is not json").unwrap();
        let store = Store::load(&path);
        assert!(store.read(|s| s.favorites.is_empty()));
        assert!(path.with_extension("corrupt").exists());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("corrupt"));
    }

    #[test]
    fn write_returns_the_closure_value() {
        let path = temp_path("retval");
        let store = Store::load(&path);
        let added = store.write(|s| s.toggle_favorite(ID_A)).unwrap();
        assert_eq!(added, Some(true));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn no_temp_file_is_left_behind() {
        let path = temp_path("tmpfile");
        let store = Store::load(&path);
        store.write(|s| s.toggle_favorite(ID_A)).unwrap();
        assert!(!path.with_extension("json.tmp").exists());
        std::fs::remove_file(&path).unwrap();
    }
}
