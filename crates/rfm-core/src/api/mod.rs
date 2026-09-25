//! The reforgermods.net API client layer.
//!
//! HTTP lives here and nowhere else. Tauri commands and the UI never build
//! requests themselves, so timeouts, retries, caching and error mapping have
//! exactly one implementation.

mod client;
mod error;
pub mod models;
mod query;

pub use client::{ApiClient, DEFAULT_BASE_URL};
pub use error::{ApiError, ApiErrorBody, ApiErrorDetail};
pub use query::{ClientPlatform, HistoryRange, ServerQuery, ServerSort, MAX_PER_PAGE};
