//! The Tauri command surface.
//!
//! This is the only boundary the UI can call across, so it is kept deliberately
//! small. Commands orchestrate; they contain no HTTP, no URI building and no
//! persistence rules of their own — those live in `rfm-core` where they are
//! unit-tested.
//!
//! Untrusted API text never reaches an OS call here. The two commands that hand
//! a URL to the operating system ([`join_server`] and [`open_server_page`]) both
//! build it from a validated [`rfm_core::ServerId`], never from an API-supplied
//! URL field.

use serde::Serialize;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use rfm_core::api::models::{
    DatasetMeta, FleetNow, LauncherStatus, PingSite, ServerDetail, ServerHistory, ServerSummary,
};
use rfm_core::api::{ApiError, HistoryRange, ServerQuery};
use rfm_core::launch::{steam_join_uri, steam_launch_uri, LaunchError};
use rfm_core::modscan::FilesystemInventory;
use rfm_core::ping::{self, PingResult};
use rfm_core::presence::Activity;
use rfm_core::readiness::{self, ReadinessReport, Unavailable};
use rfm_core::store::{now_unix, LaunchRecord, LocalState, RecentServer, Settings, StoreError};
use rfm_core::ServerId;
use std::collections::HashMap;

use crate::state::AppState;

/// An error the UI can branch on.
///
/// `kind` is a stable slug; `message` is display text. Nothing here leaks a file
/// path or an internal type name.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub kind: String,
    pub message: String,
    /// Set for rate limiting, so the UI can say how long to wait.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
}

impl From<ApiError> for CommandError {
    fn from(err: ApiError) -> Self {
        let retry_after_secs = match &err {
            ApiError::RateLimited { retry_after } => *retry_after,
            _ => None,
        };
        Self {
            kind: err.kind().to_string(),
            message: err.to_string(),
            retry_after_secs,
        }
    }
}

impl From<LaunchError> for CommandError {
    fn from(err: LaunchError) -> Self {
        Self {
            kind: "invalid_server_id".into(),
            message: err.to_string(),
            retry_after_secs: None,
        }
    }
}

impl From<StoreError> for CommandError {
    fn from(err: StoreError) -> Self {
        Self {
            kind: "storage".into(),
            // Deliberately not including the path: it names the user's profile.
            message: "Could not save launcher settings.".into(),
            retry_after_secs: None,
        }
        .tap_log(&err)
    }
}

impl CommandError {
    /// Logs the underlying cause to stderr while returning the sanitised error.
    fn tap_log(self, cause: &dyn std::error::Error) -> Self {
        eprintln!("[reforgermods-launcher] {}: {cause}", self.kind);
        self
    }

    fn other(kind: &str, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            retry_after_secs: None,
        }
    }
}

type CommandResult<T> = Result<T, CommandError>;

/// One page of the server browser.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerListResult {
    pub servers: Vec<ServerSummary>,
    /// Total servers the API matched, before client-side filtering.
    pub total_servers: u32,
    pub total_pages: u32,
    pub current_page: u32,
    /// How many of the fetched servers the client-side filters removed, so the
    /// UI can say so instead of silently showing fewer rows.
    pub filtered_out: u32,
    pub dataset: DatasetMeta,
}

/// Diagnostic view of the install identity, for debug mode.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallIdentity {
    pub id: String,
    /// True when the identifier survives a reinstall.
    pub machine_derived: bool,
}

/// Everything the UI needs from local storage in one call at startup.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalStateDto {
    pub settings: Settings,
    pub favorites: Vec<String>,
    pub recents: Vec<RecentServer>,
    pub history: Vec<LaunchRecord>,
}

impl From<&LocalState> for LocalStateDto {
    fn from(s: &LocalState) -> Self {
        Self {
            settings: s.settings.clone(),
            favorites: s.favorites.clone(),
            recents: s.recents.clone(),
            history: s.history.clone(),
        }
    }
}

/// `GET /v2/servers` with the browser's current filters.
#[tauri::command]
pub async fn list_servers(
    state: State<'_, AppState>,
    query: ServerQuery,
) -> CommandResult<ServerListResult> {
    let page = state.client().servers(&query).await?;
    state.remember_dataset(page.dataset.clone());
    state.publish_presence(Activity::Browsing {
        online_servers: page.dataset.online_server_count,
    });

    let fetched = page.data.len() as u32;
    let servers: Vec<ServerSummary> = page
        .data
        .into_iter()
        .filter(|s| query.retains(s.players))
        .collect();

    Ok(ServerListResult {
        filtered_out: fetched - servers.len() as u32,
        servers,
        total_servers: page.meta.total_servers,
        total_pages: page.meta.total_pages,
        current_page: page.meta.current_page,
        dataset: page.dataset,
    })
}

/// `GET /v2/servers/{id}`, and record the server in recents.
#[tauri::command]
pub async fn get_server(state: State<'_, AppState>, id: String) -> CommandResult<ServerDetail> {
    let id = ServerId::parse(&id)?;
    let response = state.client().server(&id).await?;
    state.remember_dataset(response.dataset.clone());

    let name = response.server.summary.name.clone();
    state.publish_presence(Activity::Viewing {
        server_name: &name,
        players: response.server.summary.players,
        max_players: response.server.summary.max_players,
    });
    // A failure to persist a recents entry must not fail the lookup the user asked for.
    let _ = state
        .store
        .write(|s| s.touch_recent(id.as_str(), &name, now_unix()));

    Ok(response.server)
}

/// `GET /v2/servers/{id}/history` for one range.
#[tauri::command]
pub async fn get_server_history(
    state: State<'_, AppState>,
    id: String,
    range: HistoryRange,
) -> CommandResult<ServerHistory> {
    let id = ServerId::parse(&id)?;
    Ok(state.client().server_history(&id, range).await?)
}

/// Compares a server's required mods against the local Arma Reforger addon store.
///
/// Reporting only. There is no verified way to install Reforger Workshop content
/// from outside the game, so this command has no counterpart that acts on the
/// result — see `docs/mod-readiness.md`.
///
/// The server detail comes from the client's cache in the common case, because
/// the inspector has just fetched it, so this costs a directory scan and no HTTP.
#[tauri::command]
pub async fn get_mod_readiness(
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<ReadinessReport> {
    let id = ServerId::parse(&id)?;
    let detail = state.client().server(&id).await?;
    let mods = detail.server.mods;

    // Reading a few hundred small files is blocking IO; keep it off the async
    // runtime's worker threads.
    tokio::task::spawn_blocking(move || match FilesystemInventory::discover() {
        Ok(inventory) => readiness::report(&mods, &inventory),
        Err(_) => ReadinessReport::unavailable(Unavailable::NoAddonsDirectory),
    })
    .await
    .map_err(|err| {
        CommandError::other("scan_failed", "Could not check locally installed mods.").tap_log(&err)
    })
}

/// Measures round-trip latency to a server by ICMP echo.
///
/// The address comes from the API and is validated by
/// [`rfm_core::ping::parse_target`] before a packet is sent: literal public IPs
/// only, so remote data cannot aim the launcher at a hostname of its choosing
/// or at the user's own network.
///
/// Never errors on an unreachable host — it returns a result that states why
/// there is no figure, because "no reply" is information, not a failure.
#[tauri::command]
pub async fn ping_server(state: State<'_, AppState>, id: String) -> CommandResult<PingResult> {
    let id = ServerId::parse(&id)?;
    // Served from cache in the common case: the inspector just fetched it.
    let detail = state.client().server(&id).await?;
    Ok(ping::measure(
        detail.server.host.as_deref(),
        ping::DEFAULT_ATTEMPTS,
        ping::DEFAULT_TIMEOUT,
    )
    .await)
}

/// Measures latency to every published ping site, keyed by site id.
///
/// This is what makes a ping column in the browser possible. `/v2/servers` does
/// not publish a per-server address — only `/v2/servers/{id}` does — so pinging
/// a 100-row page would cost 100 detail requests against a 60/minute limit.
/// Every row does carry `pingSiteId`, and there are only eight sites, so eight
/// probes cover the whole list.
///
/// The figure is therefore latency to the server's *region*, not to the server.
/// Measured against real servers the two agree within roughly 20 ms, which is
/// enough to sort by and is what the game's own browser shows. The inspector
/// still measures the selected server exactly.
///
/// Sites are probed concurrently; one unreachable site cannot hold up the rest.
#[tauri::command]
pub async fn ping_regions(
    state: State<'_, AppState>,
) -> CommandResult<HashMap<String, PingResult>> {
    let sites = state.client().ping_sites().await?.sites;

    let probes: Vec<_> = sites
        .into_iter()
        .filter_map(|site| {
            let address = site.address?;
            Some(tokio::spawn(async move {
                let result =
                    ping::measure_address(&address, ping::DEFAULT_ATTEMPTS, ping::DEFAULT_TIMEOUT)
                        .await;
                (site.id, result)
            }))
        })
        .collect();

    let mut out = HashMap::with_capacity(probes.len());
    for probe in probes {
        if let Ok((id, result)) = probe.await {
            out.insert(id, result);
        }
    }
    Ok(out)
}

/// Most servers resolvable in one call. Each costs a request, and the public
/// limit is 60/minute; favorites and recents are both bounded below this.
const MAX_RESOLVE: usize = 40;
/// Requests in flight while resolving. Enough to feel instant, low enough not
/// to burst the rate limiter.
const RESOLVE_CONCURRENCY: usize = 4;

/// Resolves a list of server ids to current summaries.
///
/// Backs the favorites and recents views, which hold ids rather than servers and
/// so cannot render a live table from local state alone. Results keep the input
/// order; ids that no longer resolve are dropped rather than failing the call,
/// because a favorite disappearing from the index is normal and the rest of the
/// list is still useful.
///
/// `/v2/servers` cannot filter by id, so this is one detail request per server.
/// That is affordable only because both lists are small — hence [`MAX_RESOLVE`].
#[tauri::command]
pub async fn get_servers(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> CommandResult<Vec<ServerSummary>> {
    let wanted: Vec<ServerId> = ids
        .iter()
        .filter_map(|id| ServerId::parse(id).ok())
        .take(MAX_RESOLVE)
        .collect();

    let limit = std::sync::Arc::new(tokio::sync::Semaphore::new(RESOLVE_CONCURRENCY));
    let mut tasks = Vec::with_capacity(wanted.len());
    for id in wanted {
        let client = state.client();
        let limit = limit.clone();
        tasks.push(tokio::spawn(async move {
            let _permit = limit.acquire().await.ok()?;
            client.server(&id).await.ok().map(|r| r.server.summary)
        }));
    }

    let mut out = Vec::with_capacity(tasks.len());
    for task in tasks {
        if let Ok(Some(summary)) = task.await {
            out.push(summary);
        }
    }
    Ok(out)
}

/// Players online across the whole fleet, for the header.
#[tauri::command]
pub async fn get_fleet_now(state: State<'_, AppState>) -> CommandResult<FleetNow> {
    Ok(state.client().fleet_now().await?.now)
}

/// Ping sites, used to label regions in the filter bar.
#[tauri::command]
pub async fn list_ping_sites(state: State<'_, AppState>) -> CommandResult<Vec<PingSite>> {
    Ok(state.client().ping_sites().await?.sites)
}

/// The compact API status for the header. Never fails; an unreachable API
/// produces an offline reading.
#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> CommandResult<LauncherStatus> {
    let hint = state.last_dataset();
    // `launcher_status` absorbs every failure into an offline/unknown reading,
    // so this is infallible; the Result is only Tauri's async command contract.
    Ok(state.client().launcher_status(hint).await)
}

/// Joins a server directly through Steam.
///
/// The id is validated first, so the URI handed to the OS contains nothing but
/// hex digits and dashes. `name` is only ever written to the local history file.
#[tauri::command]
pub async fn join_server(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> CommandResult<String> {
    let id = ServerId::parse(&id)?;

    // steam://run/<appid>//<payload>/ — the empty segment marks the payload as a
    // launch argument. See `rfm_core::launch` for why the single-slash form
    // starts the game without joining.
    let uri = steam_join_uri(&id);
    state.publish_presence(Activity::Joining { server_name: &name });
    let result = app.opener().open_url(&uri, None::<&str>);

    let succeeded = result.is_ok();
    let _ = state
        .store
        .write(|s| s.record_launch(id.as_str(), &name, succeeded, now_unix()));

    match result {
        Ok(()) => Ok(uri),
        Err(err) => Err(CommandError::other(
            "launch_failed",
            "Could not hand the join request to Steam. Is Steam installed?",
        )
        .tap_log(&err)),
    }
}

/// Starts Arma Reforger without joining a server.
#[tauri::command]
pub async fn launch_game(app: tauri::AppHandle) -> CommandResult<()> {
    app.opener()
        .open_url(steam_launch_uri(), None::<&str>)
        .map_err(|err| {
            CommandError::other(
                "launch_failed",
                "Could not start Arma Reforger through Steam.",
            )
            .tap_log(&err)
        })
}

/// Opens a server's page on reforgermods.net in the default browser.
///
/// The URL is built from the validated id rather than from the API's `modURL`
/// field, so a hostile API response cannot choose the destination.
#[tauri::command]
pub async fn open_server_page(app: tauri::AppHandle, id: String) -> CommandResult<()> {
    let id = ServerId::parse(&id)?;
    let url = format!("https://reforgermods.net/servers/{id}");
    app.opener().open_url(&url, None::<&str>).map_err(|err| {
        CommandError::other("open_failed", "Could not open the reforgermods.net page.")
            .tap_log(&err)
    })
}

/// Opens a Discord invite in the default browser.
///
/// Takes the invite **code**, never a URL: the code is re-validated here and the
/// URL is rebuilt, so text in a server name cannot choose the destination even
/// if the UI's own extraction were wrong.
#[tauri::command]
pub async fn open_discord_invite(app: tauri::AppHandle, code: String) -> CommandResult<()> {
    let valid = (2..=64).contains(&code.len())
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !valid {
        return Err(CommandError::other(
            "invalid_invite",
            "Not a valid Discord invite.",
        ));
    }
    let url = format!("https://discord.gg/{code}");
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|err| CommandError::other("open_failed", "Could not open Discord.").tap_log(&err))
}

/// Settings, favorites, recents and history, read once at startup.
#[tauri::command]
pub fn get_local_state(state: State<'_, AppState>) -> LocalStateDto {
    state.store.read(|state| LocalStateDto::from(state))
}

/// Adds or removes a favorite. Returns the new state.
#[tauri::command]
pub fn toggle_favorite(state: State<'_, AppState>, id: String) -> CommandResult<bool> {
    state
        .store
        .write(|s| s.toggle_favorite(&id))?
        .ok_or_else(|| CommandError::other("invalid_server_id", "Not a valid server id."))
}

/// Replaces application settings, re-pointing the API client when the base URL
/// changed. Returns the settings as stored, after clamping.
#[tauri::command]
pub fn update_settings(state: State<'_, AppState>, settings: Settings) -> CommandResult<Settings> {
    let stored = state.store.write(|s| {
        s.settings = settings;
        s.settings.sanitise();
        s.settings.clone()
    })?;
    // Applies a presence opt-out immediately rather than at the next refresh.
    if !stored.discord_presence {
        state.publish_presence(Activity::Browsing { online_servers: 0 });
    }
    Ok(stored)
}

/// Diagnostic view of the installation identity. Debug mode only.
///
/// Returns an empty id outside debug mode so the value is not exposed to the
/// UI — and therefore not to anything reading the window — in normal use.
#[tauri::command]
pub fn get_install_identity(state: State<'_, AppState>) -> InstallIdentity {
    if !crate::debug_mode() {
        return InstallIdentity::default();
    }
    InstallIdentity {
        id: state.store.install_id(),
        machine_derived: rfm_core::identity::is_machine_derived(),
    }
}

/// Whether this session is running in debug mode.
///
/// Gates developer-facing detail in the UI, such as the install identifier.
/// Enabled with `RFM_DEBUG=1`.
#[tauri::command]
pub fn is_debug_mode() -> bool {
    crate::debug_mode()
}

/// Clears recents and launch history. Favorites are left alone.
#[tauri::command]
pub fn clear_history(state: State<'_, AppState>) -> CommandResult<()> {
    state.store.write(|s| {
        s.recents.clear();
        s.history.clear();
    })?;
    Ok(())
}
