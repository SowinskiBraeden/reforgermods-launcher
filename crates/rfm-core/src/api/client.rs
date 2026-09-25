//! HTTP client for the reforgermods.net public API.
//!
//! All network access in the launcher goes through this type. Tauri commands
//! call it; nothing else constructs requests.
//!
//! The launcher is a public, untrusted client. It sends no credentials and no
//! API key — only a `User-Agent` identifying the build. Do not add privileged
//! headers here: the binary ships to users and anything embedded in it is
//! readable.

use std::sync::Arc;
use std::time::Duration;

use crate::api::error::{ApiError, ApiErrorBody};
use crate::api::models::{
    DatasetMeta, FleetHistory, LauncherStatus, PingSitesResponse, ServerDetailResponse,
    ServerHistory, ServersPage, StatusReport,
};
use crate::api::query::{HistoryRange, ServerQuery};
use crate::cache::{ResponseCache, Ttl};
use crate::launch::ServerId;

/// Production API base URL. The launcher is powered by this API and ships
/// pointed at it.
pub const DEFAULT_BASE_URL: &str = "https://api.reforgermods.net";

/// Development-only override for the API origin.
///
/// Deliberately an environment variable rather than a setting: a value in the
/// settings file could be edited to redirect every request — including the
/// install identifier — at an arbitrary host, and nothing in the product needs
/// end users to change it.
pub const BASE_URL_ENV: &str = "RFM_API_BASE_URL";

/// Header carrying the anonymous installation identifier.
///
/// Lets the API count distinct installs rather than only requests. See
/// `docs/client-identification.md`.
pub const INSTALL_ID_HEADER: &str = "X-Launcher-Install-Id";

/// Per-request deadline, including connection setup and body read.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
/// Connection-only deadline, so an unreachable host fails fast rather than
/// burning the whole request budget.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Attempts beyond the first. Bounded on purpose: there is no retry loop here
/// that can run indefinitely.
const MAX_RETRIES: u32 = 2;
/// Base delay for exponential backoff between retries.
const RETRY_BACKOFF: Duration = Duration::from_millis(250);

/// A client for the reforgermods.net v2 API.
///
/// Cheap to clone: the inner `reqwest::Client` and cache are shared.
#[derive(Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    base_url: Arc<str>,
    cache: Arc<ResponseCache>,
}

/// The API origin this build talks to: production, unless `RFM_API_BASE_URL`
/// overrides it for development.
pub fn configured_base_url() -> String {
    std::env::var(BASE_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

impl ApiClient {
    /// Builds a client for the configured origin, identifying as `install_id`.
    ///
    /// `install_id` is the anonymous per-install identifier from
    /// [`crate::store::Store::install_id`]. An empty or malformed value is
    /// simply not sent, so a corrupted state file degrades to an unidentified
    /// client rather than sending garbage.
    pub fn new(install_id: &str) -> Result<Self, ApiError> {
        Self::with_base_url(&configured_base_url(), install_id)
    }

    /// Builds a client against an explicit origin. Tests and the dev override
    /// use this; product code calls [`ApiClient::new`].
    pub fn with_base_url(base_url: &str, install_id: &str) -> Result<Self, ApiError> {
        let base_url = normalise_base_url(base_url)?;

        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(value) = install_id_header_value(install_id) {
            headers.insert(INSTALL_ID_HEADER, value);
        }

        let builder = reqwest::Client::builder().default_headers(headers);
        // On Windows the TLS backend is schannel, which uses the machine's own
        // trust store. See the target-specific reqwest features in Cargo.toml.
        #[cfg(windows)]
        let builder = builder.use_native_tls();

        let http = builder
            .user_agent(crate::user_agent())
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            // The launcher makes a handful of requests to one host; a small pool
            // kept alive is what keeps idle resource use low.
            .pool_max_idle_per_host(4)
            .build()
            .map_err(ApiError::Build)?;
        Ok(Self {
            http,
            base_url: base_url.into(),
            cache: Arc::new(ResponseCache::default()),
        })
    }

    /// The base URL this client targets.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Drops every cached response. Called when settings change the base URL.
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// `GET /v2/servers` with the given filters applied.
    ///
    /// Dropping the returned future cancels the request; Tauri does this when a
    /// command is aborted, which is how a superseded search stops in flight.
    pub async fn servers(&self, query: &ServerQuery) -> Result<ServersPage, ApiError> {
        let path = "/v2/servers";
        let body = self
            .get_text(path, &query.to_params(), Ttl::SNAPSHOT)
            .await?;
        decode(&body)
    }

    /// `GET /v2/servers/{id}`. The id is validated before it reaches the URL.
    pub async fn server(&self, id: &ServerId) -> Result<ServerDetailResponse, ApiError> {
        let path = format!("/v2/servers/{id}");
        let body = self.get_text(&path, &[], Ttl::SNAPSHOT).await?;
        decode(&body)
    }

    /// `GET /v2/servers/{id}/history` for one range.
    ///
    /// Returns an empty `points` array for a server with no recorded history,
    /// which is a normal state and not an error.
    pub async fn server_history(
        &self,
        id: &ServerId,
        range: HistoryRange,
    ) -> Result<ServerHistory, ApiError> {
        let path = format!("/v2/servers/{id}/history");
        let params = [("range", range.as_param().to_string())];
        let body = self.get_text(&path, &params, Ttl::HISTORY).await?;
        decode(&body)
    }

    /// Fleet-wide population, for the header's player total.
    ///
    /// The endpoint also returns a full time series; the launcher reads only the
    /// `now` block. The narrowest range is requested to keep the response small.
    pub async fn fleet_now(&self) -> Result<FleetHistory, ApiError> {
        let body = self
            .get_text(
                "/v2/servers/history",
                &[("range", "6h".to_string())],
                Ttl::HISTORY,
            )
            .await?;
        decode(&body)
    }

    /// `GET /v2/servers/ping-sites`. Used to label regions; cached for an hour.
    pub async fn ping_sites(&self) -> Result<PingSitesResponse, ApiError> {
        let body = self
            .get_text("/v2/servers/ping-sites", &[], Ttl::STABLE)
            .await?;
        decode(&body)
    }

    /// `GET /v2/status`, the public status contract.
    ///
    /// Returns [`ApiError::EndpointUnavailable`] when the deployment does not
    /// serve the route, which is the current state of api.reforgermods.net.
    pub async fn status(&self) -> Result<StatusReport, ApiError> {
        let path = "/v2/status";
        // Deliberately uncached: a status reading must be current.
        let body = self.request_text(path, &[]).await.map_err(|e| match e {
            // A 404 on a fixed route means the route is not deployed, not that a
            // resource is missing.
            ApiError::NotFound { .. }
            | ApiError::Api {
                status: 404 | 501, ..
            } => ApiError::EndpointUnavailable {
                path: path.to_string(),
            },
            other => other,
        })?;
        decode(&body)
    }

    /// The compact status the launcher header renders.
    ///
    /// This never fails: an unreachable or partially available API produces an
    /// unknown/offline reading rather than an error the UI has to handle. It also
    /// does not reimplement any backend health logic — it only reports what the
    /// public contract returned.
    ///
    /// `dataset_hint` is the [`DatasetMeta`] from the most recent server
    /// response. Every server response carries snapshot freshness, so the
    /// "server data updated Ns ago" line costs no extra request.
    pub async fn launcher_status(&self, dataset_hint: Option<DatasetMeta>) -> LauncherStatus {
        match self.status().await {
            Ok(report) => LauncherStatus {
                api_reachable: true,
                dataset: report.dataset.or(dataset_hint),
                reported_status: report.status,
                note: None,
            },
            Err(ApiError::EndpointUnavailable { .. }) => {
                // Fall back to the liveness probe, which is deployed.
                let reachable = self.request_text("/v2/health", &[]).await.is_ok();
                LauncherStatus {
                    api_reachable: reachable,
                    dataset: dataset_hint,
                    reported_status: None,
                    note: Some(
                        "This API deployment does not serve /v2/status; \
                         freshness is read from the server dataset."
                            .into(),
                    ),
                }
            }
            Err(err) => LauncherStatus {
                api_reachable: false,
                dataset: dataset_hint,
                reported_status: None,
                note: Some(err.to_string()),
            },
        }
    }

    /// Cache-aware GET returning the raw body.
    async fn get_text(
        &self,
        path: &str,
        params: &[(&'static str, String)],
        ttl: Ttl,
    ) -> Result<String, ApiError> {
        let key = cache_key(path, params);
        if let Some(hit) = self.cache.get(&key, ttl) {
            return Ok(hit);
        }
        let body = self.request_text(path, params).await?;
        self.cache.put(&key, body.clone());
        Ok(body)
    }

    /// GET with bounded retries. No caching, no deserialisation.
    async fn request_text(
        &self,
        path: &str,
        params: &[(&'static str, String)],
    ) -> Result<String, ApiError> {
        let url = format!("{}{path}", self.base_url);
        let mut attempt = 0;
        loop {
            let result = self.attempt(&url, params).await;
            match result {
                Ok(body) => return Ok(body),
                Err(err) if attempt < MAX_RETRIES && err.is_retryable() => {
                    // Fixed ceiling on attempts; this cannot spin.
                    tokio::time::sleep(RETRY_BACKOFF * 2u32.pow(attempt)).await;
                    attempt += 1;
                }
                Err(err) => return Err(err),
            }
        }
    }

    /// One HTTP round trip, mapped onto [`ApiError`].
    async fn attempt(
        &self,
        url: &str,
        params: &[(&'static str, String)],
    ) -> Result<String, ApiError> {
        let response = self.http.get(url).query(params).send().await.map_err(|e| {
            if e.is_timeout() {
                ApiError::Timeout
            } else {
                ApiError::Offline(e)
            }
        })?;

        let status = response.status();
        if status.is_success() {
            return response.text().await.map_err(|e| {
                if e.is_timeout() {
                    ApiError::Timeout
                } else {
                    ApiError::Offline(e)
                }
            });
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok());
            return Err(ApiError::RateLimited { retry_after });
        }

        // Read the documented error envelope when present; fall back to the
        // status code when the body is something else (a proxy error page).
        let body = response.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<ApiErrorBody>(&body)
            .ok()
            .map(|b| b.error);
        let (code, message, request_id) = match detail {
            Some(d) => (d.code, d.message, d.request_id),
            None => (
                String::new(),
                format!("API returned HTTP {}", status.as_u16()),
                String::new(),
            ),
        };

        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(ApiError::NotFound {
                message: if message.is_empty() {
                    "Not found.".into()
                } else {
                    message
                },
            });
        }

        Err(ApiError::Api {
            status: status.as_u16(),
            code,
            message,
            request_id,
        })
    }
}

/// Deserialises a response body into `T`.
fn decode<T: serde::de::DeserializeOwned>(body: &str) -> Result<T, ApiError> {
    serde_json::from_str(body).map_err(ApiError::Decode)
}

/// Cache key for a request. Parameter order comes from
/// [`ServerQuery::to_params`], which is stable, so equal queries collide.
fn cache_key(path: &str, params: &[(&'static str, String)]) -> String {
    let mut key = String::with_capacity(path.len() + params.len() * 16);
    key.push_str(path);
    for (i, (name, value)) in params.iter().enumerate() {
        key.push(if i == 0 { '?' } else { '&' });
        key.push_str(name);
        key.push('=');
        key.push_str(value);
    }
    key
}

/// The header value for `install_id`, or `None` when it is not a usable id.
///
/// Validated against the same UUID shape the store writes, which also
/// guarantees the value is header-safe ASCII.
fn install_id_header_value(install_id: &str) -> Option<reqwest::header::HeaderValue> {
    let id = install_id.trim();
    if crate::launch::ServerId::parse(id).is_err() {
        return None;
    }
    reqwest::header::HeaderValue::from_str(id).ok()
}

/// Validates and normalises a base URL: absolute http(s), no trailing slash.
///
/// Rejecting non-HTTP schemes here stops a settings file from pointing the
/// client at, say, a `file://` path.
fn normalise_base_url(raw: &str) -> Result<String, ApiError> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(ApiError::InvalidBaseUrl("base URL is empty".into()));
    }
    let rest = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .ok_or_else(|| ApiError::InvalidBaseUrl(format!("{raw:?} is not an http(s) URL")))?;
    // A host must be present and must not contain path or query separators.
    if rest.is_empty() || rest.starts_with('/') {
        return Err(ApiError::InvalidBaseUrl(format!("{raw:?} has no host")));
    }
    if rest.contains(['?', '#', ' ']) {
        return Err(ApiError::InvalidBaseUrl(format!(
            "{raw:?} must be a bare origin, without query or fragment"
        )));
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::query::ServerSort;

    #[test]
    fn default_base_url_is_production() {
        assert_eq!(DEFAULT_BASE_URL, "https://api.reforgermods.net");
    }

    const SAMPLE_INSTALL_ID: &str = "8f14e45f-ceea-467a-9c2b-1f2a3b4c5d6e";

    #[test]
    fn client_builds_against_the_production_base_url() {
        let client = ApiClient::new(SAMPLE_INSTALL_ID).unwrap();
        assert_eq!(client.base_url(), "https://api.reforgermods.net");
    }

    #[test]
    fn install_id_becomes_a_header_value() {
        let value = install_id_header_value(SAMPLE_INSTALL_ID).expect("valid id");
        assert_eq!(value.to_str().unwrap(), SAMPLE_INSTALL_ID);
    }

    #[test]
    fn a_malformed_install_id_is_not_sent_at_all() {
        // A corrupted state file must degrade to an unidentified client rather
        // than putting arbitrary text into a request header.
        for bad in [
            "",
            "   ",
            "not-a-uuid",
            "8f14e45f-ceea-467a-9c2b-1f2a3b4c5d6e
X-Injected: 1",
            "../../etc/passwd",
        ] {
            assert!(install_id_header_value(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn a_client_without_an_install_id_still_builds() {
        let client = ApiClient::new("").expect("must not fail");
        assert_eq!(client.base_url(), "https://api.reforgermods.net");
    }

    #[test]
    fn the_base_url_is_not_user_configurable() {
        // There is no setting for this; only the documented dev env override.
        assert_eq!(BASE_URL_ENV, "RFM_API_BASE_URL");
        assert_eq!(configured_base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn base_urls_are_normalised() {
        for (input, expected) in [
            (
                "https://api.reforgermods.net/",
                "https://api.reforgermods.net",
            ),
            (
                "https://api.reforgermods.net///",
                "https://api.reforgermods.net",
            ),
            ("  http://127.0.0.1:8000  ", "http://127.0.0.1:8000"),
            ("http://localhost:8000/api", "http://localhost:8000/api"),
        ] {
            assert_eq!(normalise_base_url(input).unwrap(), expected, "{input}");
        }
    }

    #[test]
    fn hostile_base_urls_are_rejected() {
        for input in [
            "",
            "   ",
            "api.reforgermods.net",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "steam://run/1874880",
            "https://",
            "http:///nohost",
            "https://host?x=1",
            "https://host#frag",
            "https://ho st",
        ] {
            assert!(
                normalise_base_url(input).is_err(),
                "should be rejected: {input:?}"
            );
        }
    }

    #[test]
    fn user_agent_names_the_launcher_and_version() {
        let ua = crate::user_agent();
        assert!(ua.starts_with("reforgermods.net-launcher/"), "{ua}");
        assert_eq!(
            ua,
            format!("reforgermods.net-launcher/{}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn cache_keys_include_every_parameter() {
        let q = ServerQuery {
            search: "everon".into(),
            sort: ServerSort::Name,
            ..Default::default()
        };
        let key = cache_key("/v2/servers", &q.to_params());
        assert_eq!(
            key,
            "/v2/servers?page=1&perPage=100&sort=name&search=everon"
        );
    }

    #[test]
    fn cache_keys_distinguish_different_queries() {
        let a = ServerQuery::default();
        let b = ServerQuery {
            search: "x".into(),
            ..Default::default()
        };
        assert_ne!(
            cache_key("/v2/servers", &a.to_params()),
            cache_key("/v2/servers", &b.to_params())
        );
    }

    #[test]
    fn cache_keys_match_for_equal_queries() {
        let a = ServerQuery {
            official: Some(true),
            min_players: 10,
            ..Default::default()
        };
        // min_players is client-side, so it must not split the cache.
        let b = ServerQuery {
            official: Some(true),
            min_players: 50,
            ..Default::default()
        };
        assert_eq!(
            cache_key("/v2/servers", &a.to_params()),
            cache_key("/v2/servers", &b.to_params())
        );
    }

    #[test]
    fn cache_key_for_a_pathless_request_has_no_question_mark() {
        assert_eq!(cache_key("/v2/health", &[]), "/v2/health");
    }

    #[test]
    fn retry_budget_is_bounded() {
        // Guards against a future edit turning the retry loop unbounded.
        assert_eq!(MAX_RETRIES, 2);
        let total: Duration = (0..MAX_RETRIES).map(|a| RETRY_BACKOFF * 2u32.pow(a)).sum();
        assert!(total < Duration::from_secs(1), "backoff total {total:?}");
    }

    #[test]
    fn server_detail_path_uses_a_validated_id() {
        let id = ServerId::parse("73e8fb7f-8789-4d88-8aa8-ea69b9aa092f").unwrap();
        assert_eq!(
            format!("/v2/servers/{id}"),
            "/v2/servers/73e8fb7f-8789-4d88-8aa8-ea69b9aa092f"
        );
    }
}
