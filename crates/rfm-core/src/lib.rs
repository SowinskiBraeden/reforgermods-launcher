//! Core logic for the reforgermods.net launcher.
//!
//! This crate deliberately has no Tauri dependency. Everything that can be
//! decided without a window lives here so it can be unit-tested directly:
//! the reforgermods.net API client, Steam launch-URI construction, local
//! persistence, response caching, and local Workshop addon detection.

pub mod api;
pub mod cache;
pub mod identity;
pub mod launch;
pub mod modscan;
pub mod ping;
pub mod presence;
pub mod readiness;
pub mod store;

pub use api::{ApiClient, ApiError};
pub use launch::{reforger_join_uri, steam_join_uri, LaunchError, ServerId};

/// User-Agent the launcher identifies itself with. The launcher is a public,
/// untrusted client: it sends no credentials, only this.
pub fn user_agent() -> String {
    format!("reforgermods.net-launcher/{}", env!("CARGO_PKG_VERSION"))
}
