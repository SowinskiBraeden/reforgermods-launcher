//! Server browser query construction.
//!
//! Only filters the live API actually supports are modelled here. `/v2/servers`
//! accepts `page`, `perPage`, `search`, `sort`, `region`, `platform`, `online`,
//! `includeOffline`, `official`, `hasMods`, `battleye` and `locked`; anything
//! else would be silently ignored by the API and would produce a filter control
//! that appears to work but does not.
//!
//! [`ServerQuery::min_players`] is the one exception and is explicitly *not*
//! sent to the API: it is applied client-side by [`ServerQuery::retains`],
//! because no server-side equivalent exists.

use serde::{Deserialize, Serialize};

/// Sort orders `/v2/servers` accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServerSort {
    /// Most players first. The useful default for a browser.
    #[default]
    Players,
    Name,
    Newest,
    LastSeen,
}

impl ServerSort {
    /// The wire value for the `sort` parameter.
    pub fn as_param(self) -> &'static str {
        match self {
            ServerSort::Players => "players",
            ServerSort::Name => "name",
            ServerSort::Newest => "newest",
            ServerSort::LastSeen => "lastSeen",
        }
    }
}

/// Client platforms the API can filter by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientPlatform {
    Pc,
    Xbox,
    Psn,
}

impl ClientPlatform {
    /// The wire value for the `platform` parameter. The API canonicalises these
    /// short tokens itself.
    pub fn as_param(self) -> &'static str {
        match self {
            ClientPlatform::Pc => "pc",
            ClientPlatform::Xbox => "xbox",
            ClientPlatform::Psn => "psn",
        }
    }
}

/// Largest page the API permits.
pub const MAX_PER_PAGE: u32 = 500;

/// A server browser query.
///
/// `None` on a tri-state field means "do not filter", which is different from
/// `Some(false)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerQuery {
    /// Free-text search over server names.
    pub search: String,
    pub sort: ServerSort,
    /// Ping-site id, e.g. `frankfurt`. See `GET /v2/servers/ping-sites`.
    pub region: Option<String>,
    pub platform: Option<ClientPlatform>,
    /// Official Bohemia servers only / community only.
    pub official: Option<bool>,
    /// Servers reporting at least one mod / none.
    pub has_mods: Option<bool>,
    pub battleye: Option<bool>,
    /// `locked` is the API's name for password-protected.
    pub locked: Option<bool>,
    /// Include servers not in the current snapshot. Off by default: an offline
    /// server cannot be joined.
    pub include_offline: bool,
    /// Client-side only. Drops servers below this player count from the loaded
    /// page; the API has no equivalent parameter.
    pub min_players: u32,
    pub page: u32,
    pub per_page: u32,
}

impl Default for ServerQuery {
    fn default() -> Self {
        Self {
            search: String::new(),
            sort: ServerSort::default(),
            region: None,
            platform: None,
            official: None,
            has_mods: None,
            battleye: None,
            locked: None,
            include_offline: false,
            min_players: 0,
            page: 1,
            per_page: 100,
        }
    }
}

impl ServerQuery {
    /// The `/v2/servers` query parameters for this query, in a stable order.
    ///
    /// Stable ordering matters: the serialised form is the cache key, so two
    /// equal queries must produce byte-identical strings.
    pub fn to_params(&self) -> Vec<(&'static str, String)> {
        let mut params: Vec<(&'static str, String)> = Vec::with_capacity(12);

        params.push(("page", self.page.max(1).to_string()));
        params.push(("perPage", self.per_page.clamp(1, MAX_PER_PAGE).to_string()));
        params.push(("sort", self.sort.as_param().to_string()));

        let search = self.search.trim();
        if !search.is_empty() {
            params.push(("search", search.to_string()));
        }
        if let Some(region) = self
            .region
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty())
        {
            params.push(("region", region.to_string()));
        }
        if let Some(platform) = self.platform {
            params.push(("platform", platform.as_param().to_string()));
        }
        // Tri-state flags: only sent when the user actually chose a side.
        for (name, value) in [
            ("official", self.official),
            ("hasMods", self.has_mods),
            ("battleye", self.battleye),
            ("locked", self.locked),
        ] {
            if let Some(value) = value {
                params.push((name, value.to_string()));
            }
        }
        if self.include_offline {
            params.push(("includeOffline", "true".to_string()));
        }
        params
    }

    /// Whether a fetched server survives the client-side filters.
    ///
    /// Only `min_players` lives here — the one filter the API cannot express.
    /// Everything else was already applied server-side, and re-checking it
    /// locally would mean two filter implementations that could disagree.
    pub fn retains(&self, players: u32) -> bool {
        players >= self.min_players
    }

    /// True when any filter beyond sort and pagination is active. Drives the
    /// "clear filters" affordance.
    pub fn has_active_filters(&self) -> bool {
        // Mirrors `to_params`: a blank search or region is not sent to the API,
        // so it must not count as an active filter either.
        !self.search.trim().is_empty()
            || self.region.as_deref().is_some_and(|r| !r.trim().is_empty())
            || self.platform.is_some()
            || self.official.is_some()
            || self.has_mods.is_some()
            || self.battleye.is_some()
            || self.locked.is_some()
            || self.include_offline
            || self.min_players > 0
    }
}

/// Ranges `GET /v2/servers/{id}/history` accepts.
///
/// The API rejects anything else with `INVALID_RANGE`, so this is an enum rather
/// than a free string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryRange {
    SixHours,
    /// The API's own default.
    #[default]
    OneDay,
    ThreeDays,
    OneWeek,
    OneMonth,
    ThreeMonths,
    OneYear,
    All,
}

impl HistoryRange {
    /// The wire value for the `range` parameter.
    pub fn as_param(self) -> &'static str {
        match self {
            HistoryRange::SixHours => "6h",
            HistoryRange::OneDay => "24h",
            HistoryRange::ThreeDays => "72h",
            HistoryRange::OneWeek => "7d",
            HistoryRange::OneMonth => "30d",
            HistoryRange::ThreeMonths => "90d",
            HistoryRange::OneYear => "1y",
            HistoryRange::All => "all",
        }
    }

    /// The ranges the inspector offers. A subset: 90d, 1y and all are supported
    /// by the API but are not useful in a compact panel.
    pub const OFFERED: [HistoryRange; 4] = [
        HistoryRange::SixHours,
        HistoryRange::OneDay,
        HistoryRange::OneWeek,
        HistoryRange::OneMonth,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param_string(q: &ServerQuery) -> String {
        q.to_params()
            .into_iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&")
    }

    #[test]
    fn default_query_asks_for_the_busiest_online_servers() {
        let q = ServerQuery::default();
        assert_eq!(param_string(&q), "page=1&perPage=100&sort=players");
        // includeOffline absent means online-only, which is the API default.
        assert!(!q.has_active_filters());
    }

    #[test]
    fn unset_tristate_filters_are_omitted_entirely() {
        let q = ServerQuery::default();
        let keys: Vec<_> = q.to_params().into_iter().map(|(k, _)| k).collect();
        for absent in [
            "official",
            "hasMods",
            "battleye",
            "locked",
            "includeOffline",
        ] {
            assert!(!keys.contains(&absent), "{absent} should not be sent");
        }
    }

    #[test]
    fn false_is_distinct_from_unset() {
        let q = ServerQuery {
            official: Some(false),
            has_mods: Some(false),
            ..Default::default()
        };
        assert!(param_string(&q).contains("official=false"));
        assert!(param_string(&q).contains("hasMods=false"));
        assert!(q.has_active_filters());
    }

    #[test]
    fn all_filters_serialise_in_a_stable_order() {
        let q = ServerQuery {
            search: "  conflict  ".into(),
            sort: ServerSort::Name,
            region: Some("frankfurt".into()),
            platform: Some(ClientPlatform::Pc),
            official: Some(true),
            has_mods: Some(true),
            battleye: Some(true),
            locked: Some(false),
            include_offline: true,
            min_players: 10,
            page: 3,
            per_page: 50,
        };
        assert_eq!(
            param_string(&q),
            "page=3&perPage=50&sort=name&search=conflict&region=frankfurt&platform=pc\
             &official=true&hasMods=true&battleye=true&locked=false&includeOffline=true"
        );
        // Identical queries must serialise identically, so the cache key is stable.
        assert_eq!(param_string(&q), param_string(&q.clone()));
    }

    #[test]
    fn min_players_is_never_sent_to_the_api() {
        let q = ServerQuery {
            min_players: 64,
            ..Default::default()
        };
        assert!(!param_string(&q).contains("min"), "{}", param_string(&q));
        assert!(q.has_active_filters());
    }

    #[test]
    fn min_players_filters_client_side() {
        let q = ServerQuery {
            min_players: 10,
            ..Default::default()
        };
        assert!(!q.retains(0));
        assert!(!q.retains(9));
        assert!(q.retains(10));
        assert!(q.retains(128));
        // A zero threshold keeps everything, including empty servers.
        assert!(ServerQuery::default().retains(0));
    }

    #[test]
    fn history_ranges_match_the_api_vocabulary() {
        // The API rejects anything outside this set with INVALID_RANGE.
        let params: Vec<&str> = [
            HistoryRange::SixHours,
            HistoryRange::OneDay,
            HistoryRange::ThreeDays,
            HistoryRange::OneWeek,
            HistoryRange::OneMonth,
            HistoryRange::ThreeMonths,
            HistoryRange::OneYear,
            HistoryRange::All,
        ]
        .iter()
        .map(|r| r.as_param())
        .collect();
        assert_eq!(
            params,
            ["6h", "24h", "72h", "7d", "30d", "90d", "1y", "all"]
        );
        // The default matches the API's own default.
        assert_eq!(HistoryRange::default().as_param(), "24h");
        assert!(HistoryRange::OFFERED.contains(&HistoryRange::default()));
    }

    #[test]
    fn blank_search_and_region_are_dropped() {
        let q = ServerQuery {
            search: "   ".into(),
            region: Some("  ".into()),
            ..Default::default()
        };
        assert_eq!(param_string(&q), "page=1&perPage=100&sort=players");
        assert!(!q.has_active_filters());
    }

    #[test]
    fn pagination_is_clamped_to_api_limits() {
        let q = ServerQuery {
            page: 0,
            per_page: 9_999,
            ..Default::default()
        };
        assert!(param_string(&q).contains("page=1"));
        assert!(param_string(&q).contains(&format!("perPage={MAX_PER_PAGE}")));

        let q = ServerQuery {
            per_page: 0,
            ..Default::default()
        };
        assert!(param_string(&q).contains("perPage=1"));
    }

    #[test]
    fn sort_and_platform_params_match_the_api_vocabulary() {
        assert_eq!(ServerSort::Players.as_param(), "players");
        assert_eq!(ServerSort::LastSeen.as_param(), "lastSeen");
        assert_eq!(ServerSort::Newest.as_param(), "newest");
        assert_eq!(ServerSort::Name.as_param(), "name");
        assert_eq!(ClientPlatform::Pc.as_param(), "pc");
        assert_eq!(ClientPlatform::Xbox.as_param(), "xbox");
        assert_eq!(ClientPlatform::Psn.as_param(), "psn");
    }

    #[test]
    fn query_round_trips_through_json() {
        // The UI sends this struct across the Tauri boundary.
        let q = ServerQuery {
            search: "everon".into(),
            sort: ServerSort::LastSeen,
            platform: Some(ClientPlatform::Psn),
            locked: Some(false),
            min_players: 5,
            ..Default::default()
        };
        let json = serde_json::to_string(&q).unwrap();
        assert_eq!(serde_json::from_str::<ServerQuery>(&json).unwrap(), q);
    }

    #[test]
    fn partial_json_from_the_ui_fills_in_defaults() {
        let q: ServerQuery = serde_json::from_str(r#"{"search":"ru"}"#).unwrap();
        assert_eq!(q.search, "ru");
        assert_eq!(q.page, 1);
        assert_eq!(q.per_page, 100);
        assert_eq!(q.sort, ServerSort::Players);
    }
}
