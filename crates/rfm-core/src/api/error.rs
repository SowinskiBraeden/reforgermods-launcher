//! Error type for the reforgermods.net API client.

use serde::Deserialize;

/// The error envelope reforgermods.net returns for non-2xx responses.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorBody {
    pub error: ApiErrorDetail,
}

/// The `error` object inside [`ApiErrorBody`].
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorDetail {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
    #[serde(default, rename = "requestId")]
    pub request_id: String,
}

/// Every way an API call can fail.
///
/// Variants are deliberately distinct so the UI can choose sensible copy:
/// `Offline` and `Timeout` are "try again", `RateLimited` is "wait", `NotFound`
/// is "this server is gone", and `Api` carries a server-authored message.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// The request never reached the API (DNS, TLS, connection refused).
    #[error("could not reach the reforgermods.net API")]
    Offline(#[source] reqwest::Error),

    /// The request exceeded its deadline.
    #[error("request to the reforgermods.net API timed out")]
    Timeout,

    /// A 404 for a resource that may simply no longer be indexed.
    #[error("{message}")]
    NotFound { message: String },

    /// The requested route does not exist on this deployment. Distinct from
    /// [`ApiError::NotFound`] so callers can degrade a feature rather than
    /// reporting a missing server.
    #[error("endpoint {path} is not available on this API deployment")]
    EndpointUnavailable { path: String },

    /// 429. `retry_after` is the `Retry-After` value in seconds when supplied.
    #[error("rate limited by the reforgermods.net API")]
    RateLimited { retry_after: Option<u64> },

    /// Any other non-2xx response carrying the documented error envelope.
    #[error("{message} ({code})")]
    Api {
        status: u16,
        code: String,
        message: String,
        request_id: String,
    },

    /// A 2xx response whose body did not match the expected model.
    #[error("unexpected response shape from the reforgermods.net API")]
    Decode(#[source] serde_json::Error),

    /// The client could not be constructed (TLS backend, proxy config).
    #[error("could not build the API client")]
    Build(#[source] reqwest::Error),

    /// A configured base URL was not a usable absolute HTTP(S) URL.
    #[error("invalid API base URL: {0}")]
    InvalidBaseUrl(String),
}

impl ApiError {
    /// True when retrying the same request could plausibly succeed.
    pub fn is_retryable(&self) -> bool {
        match self {
            ApiError::Offline(_) | ApiError::Timeout => true,
            // 5xx is worth one more attempt; 4xx is not.
            ApiError::Api { status, .. } => *status >= 500,
            _ => false,
        }
    }

    /// A stable machine-readable kind, for the UI to branch on without
    /// matching on human-readable text.
    pub fn kind(&self) -> &'static str {
        match self {
            ApiError::Offline(_) => "offline",
            ApiError::Timeout => "timeout",
            ApiError::NotFound { .. } => "not_found",
            ApiError::EndpointUnavailable { .. } => "endpoint_unavailable",
            ApiError::RateLimited { .. } => "rate_limited",
            ApiError::Api { .. } => "api",
            ApiError::Decode(_) => "decode",
            ApiError::Build(_) => "build",
            ApiError::InvalidBaseUrl(_) => "config",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_envelope_parses() {
        let raw = r#"{"error":{"code":"SERVER_NOT_FOUND","message":"Server was not found in the current dataset.","requestId":"91288cfd10bc327410219cb9"}}"#;
        let body: ApiErrorBody = serde_json::from_str(raw).unwrap();
        assert_eq!(body.error.code, "SERVER_NOT_FOUND");
        assert_eq!(body.error.request_id, "91288cfd10bc327410219cb9");
    }

    #[test]
    fn error_envelope_tolerates_missing_fields() {
        let body: ApiErrorBody = serde_json::from_str(r#"{"error":{}}"#).unwrap();
        assert_eq!(body.error.code, "");
        assert_eq!(body.error.message, "");
    }

    #[test]
    fn retryability_follows_status_class() {
        let server_err = ApiError::Api {
            status: 503,
            code: "SERVERS_UNAVAILABLE".into(),
            message: "temporarily unavailable".into(),
            request_id: String::new(),
        };
        assert!(server_err.is_retryable());
        assert!(ApiError::Timeout.is_retryable());
        assert!(!ApiError::NotFound {
            message: "gone".into()
        }
        .is_retryable());
        assert!(!ApiError::RateLimited {
            retry_after: Some(5)
        }
        .is_retryable());
    }

    #[test]
    fn kinds_are_stable_slugs() {
        assert_eq!(ApiError::Timeout.kind(), "timeout");
        assert_eq!(
            ApiError::EndpointUnavailable {
                path: "/v2/status".into()
            }
            .kind(),
            "endpoint_unavailable"
        );
    }
}
