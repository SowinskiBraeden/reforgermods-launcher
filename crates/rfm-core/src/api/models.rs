//! Typed models for the reforgermods.net v2 API.
//!
//! Models are deliberately tolerant: unknown fields are ignored and absent
//! fields fall back to defaults. The API is free to add fields, and a launcher
//! that hard-fails on an unrecognised key would break on every API release.
//!
//! Every string in here is untrusted remote text. Nothing in this module
//! validates it; the only value that ever needs validating is a server id, and
//! that happens in [`crate::launch`] at the point of use.

use serde::{Deserialize, Serialize};

/// Freshness of the indexer snapshot backing the server dataset.
///
/// Present on every server response, which is why the launcher does not need a
/// separate poll to render "server data updated Ns ago".
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetMeta {
    /// True on cold start, before the indexer has produced a first snapshot.
    #[serde(default)]
    pub warming: bool,
    /// True when the API considers the snapshot too old to be current.
    #[serde(default)]
    pub stale: bool,
    /// Age of the snapshot in seconds.
    #[serde(default)]
    pub snapshot_age_seconds: f64,
    /// RFC 3339 timestamp of the collection that produced the snapshot.
    #[serde(default)]
    pub last_collection_at: Option<String>,
    /// Total servers ever indexed.
    #[serde(default)]
    pub server_count: u64,
    #[serde(default)]
    pub online_server_count: u64,
    #[serde(default)]
    pub offline_server_count: u64,
}

/// Pagination metadata for a server list page.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerListMeta {
    #[serde(default)]
    pub total_pages: u32,
    #[serde(default)]
    pub current_page: u32,
    #[serde(default)]
    pub total_servers: u32,
    #[serde(default)]
    pub shown_servers: u32,
}

/// reforgermods.net's derived ranking for a server.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerAnalytics {
    #[serde(default)]
    pub rank: Option<u32>,
    #[serde(default)]
    pub overall_score: Option<f64>,
    #[serde(default)]
    pub activity_score: Option<f64>,
    #[serde(default)]
    pub reliability_score: Option<f64>,
    #[serde(default)]
    pub confidence: Option<f64>,
}

/// A server as it appears in the browser list.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSummary {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub scenario_id: String,
    #[serde(default)]
    pub scenario_name: String,
    #[serde(default)]
    pub scenario_thumbnail_url: Option<String>,
    #[serde(default)]
    pub game_version: String,
    #[serde(default)]
    pub official: bool,
    #[serde(default)]
    pub password_protected: bool,
    #[serde(default, rename = "battlEye")]
    pub battle_eye: bool,
    #[serde(default)]
    pub cross_play: bool,
    #[serde(default)]
    pub players: u32,
    #[serde(default)]
    pub max_players: u32,
    #[serde(default)]
    pub queue: u32,
    #[serde(default)]
    pub queue_max: u32,
    /// Nitrado ping-site id, e.g. `frankfurt`. Join with [`PingSite`] for a label.
    #[serde(default)]
    pub ping_site_id: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    /// Host OS the server runs on (`LINUX`/`WINDOWS`), *not* a client platform.
    #[serde(default)]
    pub platform: Option<String>,
    /// Client platforms that may join: a subset of `pc`, `xbox`, `psn`.
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub mod_count: u32,
    #[serde(default)]
    pub online: bool,
    #[serde(default)]
    pub first_seen: Option<String>,
    #[serde(default)]
    pub last_seen: Option<String>,
    #[serde(default)]
    pub analytics: Option<ServerAnalytics>,
    /// The API's own `steam://run/...` join URI.
    ///
    /// Parsed for cross-checking only. The launcher builds its own URI from the
    /// validated id: an API-supplied URL must never be handed to the OS.
    #[serde(default)]
    pub join_url: Option<String>,
}

/// `GET /v2/servers`
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ServersPage {
    #[serde(default)]
    pub meta: ServerListMeta,
    #[serde(default)]
    pub dataset: DatasetMeta,
    #[serde(default)]
    pub data: Vec<ServerSummary>,
}

/// Aggregate player activity derived from history samples.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerActivity {
    #[serde(default)]
    pub peak24h: u32,
    #[serde(default)]
    pub avg24h: u32,
    #[serde(default)]
    pub median24h: u32,
    #[serde(default)]
    pub low24h: u32,
    #[serde(default)]
    pub peak7d: u32,
    #[serde(default)]
    pub samples24h: u32,
    #[serde(default)]
    pub tracking_since: Option<String>,
}

/// Totals for a server's reported mod set.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerModSummary {
    #[serde(default)]
    pub count: u32,
    /// Sum of sizes the API could resolve, in bytes. Not the full download size
    /// unless `unresolved_count` is zero.
    #[serde(default)]
    pub known_size: u64,
    #[serde(default)]
    pub known_size_text: Option<String>,
    /// Mods whose exact-version size could not be resolved.
    #[serde(default)]
    pub unresolved_count: u32,
}

/// One mod a server reports requiring.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerMod {
    /// Workshop mod id: 16 uppercase hex characters.
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub size_text: Option<String>,
    #[serde(default)]
    pub size_known: bool,
    /// reforgermods.net page for this mod. Untrusted; validate before opening.
    #[serde(default)]
    pub mod_url: Option<String>,
}

/// A server with connection, activity and mod detail.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerDetail {
    /// Everything the list view already knows.
    #[serde(flatten)]
    pub summary: ServerSummary,
    /// True when the server is in the current snapshot. False means "not
    /// currently listed", which is not the same as "offline".
    #[serde(default)]
    pub present: bool,
    #[serde(default)]
    pub host_type: Option<String>,
    #[serde(default)]
    pub joinable: bool,
    /// The server's address, as published by the API. Untrusted: validate with
    /// [`crate::ping::parse_target`] before doing anything with it. Absent for
    /// roughly a third of servers.
    #[serde(default)]
    pub host: Option<String>,
    /// Game port. Informational only — nothing connects to it.
    #[serde(default)]
    pub port: u16,
    /// Server-reported simulation FPS. 0 when not reported.
    #[serde(default)]
    pub fps: u32,
    #[serde(default)]
    pub activity: Option<ServerActivity>,
    #[serde(default)]
    pub mod_summary: ServerModSummary,
    #[serde(default)]
    pub mods: Vec<ServerMod>,
}

/// `GET /v2/servers/{id}`
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ServerDetailResponse {
    #[serde(default)]
    pub dataset: DatasetMeta,
    pub server: ServerDetail,
}

/// A Nitrado ping location servers are grouped under.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingSite {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub region_group: String,
    /// Hostname used for latency probes. Not contacted in milestone 1.
    #[serde(default)]
    pub address: Option<String>,
}

/// `GET /v2/servers/ping-sites`
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PingSitesResponse {
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub sites: Vec<PingSite>,
}

/// One bucket of a server's population history.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ServerHistoryPoint {
    /// Bucket start, unix seconds.
    pub t: i64,
    #[serde(default)]
    pub avg: u32,
    #[serde(default)]
    pub min: u32,
    #[serde(default)]
    pub max: u32,
    /// Slot capacity during the bucket. 0 when not recorded.
    #[serde(default)]
    pub cap: u32,
}

/// `GET /v2/servers/{id}/history`
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerHistory {
    #[serde(default)]
    pub server_id: String,
    /// Range the API actually served, which may differ from what was asked.
    #[serde(default)]
    pub range: String,
    /// Bucket width the API chose: `5m`, `hour` or `day`.
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub points: Vec<ServerHistoryPoint>,
}

/// Fleet-wide population right now, from `GET /v2/servers/history`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetNow {
    #[serde(default)]
    pub online_servers: u64,
    /// Players across every listed server.
    #[serde(default)]
    pub players: u64,
    /// Sum of slot capacity across those servers.
    #[serde(default)]
    pub capacity: u64,
    #[serde(default)]
    pub queue: u64,
}

/// `GET /v2/servers/history`, of which the launcher reads only `now`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FleetHistory {
    #[serde(default)]
    pub now: FleetNow,
}

/// `GET /v2/status`, parsed permissively.
///
/// This route is not deployed on api.reforgermods.net yet (it answers 404 as of
/// 2026-09-24), so every field is optional and the launcher treats an absent
/// route as "status unknown" rather than an error. See `docs/api-contract.md`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusReport {
    /// Overall state word when the API supplies one, e.g. `operational`.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub dataset: Option<DatasetMeta>,
}

/// What the launcher header renders. Assembled from whichever sources answered,
/// so a missing `/v2/status` degrades to an unobtrusive unknown state instead of
/// an error banner.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherStatus {
    /// True when the API answered a health or status probe.
    pub api_reachable: bool,
    /// Freshness of the server dataset, when known.
    pub dataset: Option<DatasetMeta>,
    /// Set when `/v2/status` answered and named a state.
    pub reported_status: Option<String>,
    /// Human-readable reason the status is unknown, for a tooltip.
    pub note: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixtures are real responses captured from api.reforgermods.net.
    const SERVERS_PAGE: &str = include_str!("../../tests/fixtures/servers_page.json");
    const SERVER_DETAIL: &str = include_str!("../../tests/fixtures/server_detail.json");
    const PING_SITES: &str = include_str!("../../tests/fixtures/ping_sites.json");

    #[test]
    fn servers_page_parses_from_live_fixture() {
        let page: ServersPage = serde_json::from_str(SERVERS_PAGE).unwrap();
        assert_eq!(page.data.len(), 3);
        assert_eq!(page.meta.current_page, 1);
        assert!(page.meta.total_servers > 0);
        assert!(!page.dataset.warming);
        assert!(page.dataset.last_collection_at.is_some());

        let first = &page.data[0];
        assert!(!first.id.is_empty());
        assert!(!first.name.is_empty());
        assert!(first.online);
        assert!(first.max_players > 0);
        // Sorted by players descending.
        assert!(first.players >= page.data[1].players);
    }

    #[test]
    fn battl_eye_field_name_is_mapped() {
        // The API spells this `battlEye`; a wrong rename would silently read
        // false for every server.
        let page: ServersPage = serde_json::from_str(SERVERS_PAGE).unwrap();
        let raw: serde_json::Value = serde_json::from_str(SERVERS_PAGE).unwrap();
        for (parsed, raw) in page.data.iter().zip(raw["data"].as_array().unwrap()) {
            assert_eq!(
                parsed.battle_eye,
                raw["battlEye"].as_bool().unwrap_or(false)
            );
        }
    }

    #[test]
    fn server_detail_parses_from_live_fixture() {
        let res: ServerDetailResponse = serde_json::from_str(SERVER_DETAIL).unwrap();
        let s = &res.server;
        assert_eq!(s.summary.id, "73e8fb7f-8789-4d88-8aa8-ea69b9aa092f");
        assert!(s.present);
        // Flattened summary fields must survive the flatten.
        assert!(!s.summary.name.is_empty());
        assert!(s.summary.max_players > 0);
        assert!(s.summary.mod_count > 0);
        // Detail-only fields.
        assert!(s.host.is_some(), "this fixture publishes an address");
        assert!(s.port > 0);
        assert_eq!(s.mods.len() as u32, s.mod_summary.count);
        assert!(s.activity.is_some());
        assert!(s.mods.iter().all(|m| m.id.len() == 16));
    }

    #[test]
    fn ping_sites_parse_from_live_fixture() {
        let res: PingSitesResponse = serde_json::from_str(PING_SITES).unwrap();
        assert!(res.sites.len() >= 8);
        let fra = res.sites.iter().find(|s| s.id == "frankfurt").unwrap();
        assert_eq!(fra.label, "Frankfurt");
        assert_eq!(fra.region_group, "Europe");
    }

    #[test]
    fn unknown_fields_are_ignored() {
        // A future API release adding fields must not break the launcher.
        let raw = r#"{"meta":{"currentPage":2,"brandNew":1},"dataset":{},"data":[
            {"id":"a","name":"n","somethingAdded":{"deep":true}}
        ],"extraTopLevel":"x"}"#;
        let page: ServersPage = serde_json::from_str(raw).unwrap();
        assert_eq!(page.meta.current_page, 2);
        assert_eq!(page.data[0].id, "a");
    }

    #[test]
    fn missing_optional_fields_fall_back_to_defaults() {
        let page: ServersPage = serde_json::from_str(r#"{"data":[{"id":"a"}]}"#).unwrap();
        let s = &page.data[0];
        assert_eq!(s.players, 0);
        assert!(!s.official);
        assert!(s.platforms.is_empty());
        assert!(s.ping_site_id.is_none());
        assert!(s.analytics.is_none());
    }

    #[test]
    fn an_absent_host_is_none_rather_than_an_empty_string() {
        // About a third of servers publish no address; the ping code branches
        // on None, so it must not arrive as Some("").
        let res: ServerDetailResponse = serde_json::from_str(r#"{"server":{"id":"a"}}"#).unwrap();
        assert!(res.server.host.is_none());
        assert_eq!(res.server.port, 0);
    }

    #[test]
    fn history_parses_from_live_fixture() {
        const HISTORY: &str = include_str!("../../tests/fixtures/server_history.json");
        let h: ServerHistory = serde_json::from_str(HISTORY).unwrap();
        assert_eq!(h.range, "24h");
        assert_eq!(h.bucket, "5m");
        assert!(h.points.len() > 100, "24h of 5m buckets");
        // Buckets must be ordered, or the chart draws backwards.
        assert!(h.points.windows(2).all(|w| w[0].t <= w[1].t));
        for point in &h.points {
            assert!(
                point.min <= point.avg && point.avg <= point.max,
                "{point:?}"
            );
            assert!(point.t > 1_700_000_000, "unix seconds expected");
        }
    }

    #[test]
    fn history_tolerates_an_empty_series() {
        // A server with no recorded history returns an empty array, not an error.
        let h: ServerHistory =
            serde_json::from_str(r#"{"serverId":"x","range":"7d","bucket":"hour"}"#).unwrap();
        assert!(h.points.is_empty());
    }

    #[test]
    fn status_report_tolerates_an_unknown_shape() {
        // The contract is not published yet; parsing must not hard-fail.
        let r: StatusReport = serde_json::from_str(r#"{"whatever":true}"#).unwrap();
        assert!(r.status.is_none());
        let r: StatusReport =
            serde_json::from_str(r#"{"status":"operational","dataset":{"stale":false}}"#).unwrap();
        assert_eq!(r.status.as_deref(), Some("operational"));
        assert!(!r.dataset.unwrap().stale);
    }
}
