//! Shared application state.

use std::sync::{Mutex, RwLock};

use rfm_core::api::models::DatasetMeta;
use rfm_core::presence::Activity;
use rfm_core::store::Store;

use crate::presence::PresenceHandle;
use rfm_core::{ApiClient, ApiError};

/// State held for the lifetime of the application.
pub struct AppState {
    /// Behind an `RwLock` because changing the API base URL in settings swaps the
    /// client, while every request only needs read access.
    client: RwLock<ApiClient>,
    /// Persisted settings, favorites, recents and launch history.
    pub store: Store,
    /// Discord Rich Presence publisher. Inert when no application id is set.
    presence: PresenceHandle,
    /// Snapshot freshness from the most recent server response.
    ///
    /// Every `/v2/servers` response carries this, so the header can show
    /// "server data updated Ns ago" without its own polling loop.
    last_dataset: Mutex<Option<DatasetMeta>>,
}

impl AppState {
    /// Builds state with a client for the configured API origin, identifying
    /// with the store's anonymous install id.
    pub fn new(store: Store) -> Result<Self, ApiError> {
        let client = ApiClient::new(&store.install_id())?;
        Ok(Self {
            client: RwLock::new(client),
            store,
            presence: PresenceHandle::start(),
            last_dataset: Mutex::new(None),
        })
    }

    /// A clone of the current client, so requests do not hold the lock while
    /// awaiting the network.
    pub fn client(&self) -> ApiClient {
        self.client
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Publishes `activity` to Discord, if the user has it enabled.
    ///
    /// Silently does nothing when presence is off or unconfigured; this is a
    /// decorative feature and must never interfere with a user action.
    pub fn publish_presence(&self, activity: Activity<'_>) {
        if !self.presence.is_enabled() {
            return;
        }
        if self.store.read(|s| s.settings.discord_presence) {
            self.presence.update(activity.to_presence());
        } else {
            self.presence.clear();
        }
    }

    /// Records snapshot freshness seen on a server response.
    pub fn remember_dataset(&self, dataset: DatasetMeta) {
        *self.last_dataset.lock().unwrap_or_else(|e| e.into_inner()) = Some(dataset);
    }

    /// The most recently seen snapshot freshness, if any.
    pub fn last_dataset(&self) -> Option<DatasetMeta> {
        self.last_dataset
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}
