//! Discord Rich Presence transport.
//!
//! Discord's IPC is a blocking local socket, and Discord may not be running at
//! all, so this owns one background thread and talks to it over a channel.
//! Nothing on the UI path ever blocks on Discord, and a missing or restarted
//! Discord is a no-op rather than an error the user sees.
//!
//! Payload text is built in `rfm_core::presence`, where the length and
//! sanitisation rules are tested.

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use discord_rich_presence::error::Error as DiscordError;
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use rfm_core::presence::Presence;

/// Discord application id. Determines the name shown as the game being played.
///
/// Empty by default: without a real application this build must not attempt to
/// connect. Set `RFM_DISCORD_APP_ID` at runtime, or replace this constant once
/// the application exists. See `docs/discord-presence.md`.
const DEFAULT_APP_ID: &str = "1552757014474784898";

/// Environment override for the application id.
const APP_ID_ENV: &str = "RFM_DISCORD_APP_ID";

/// Artwork key uploaded to the Discord application's Rich Presence assets.
const LARGE_IMAGE_KEY: &str = "logo";

/// How long to wait before retrying a failed connection. Discord being closed
/// is the normal case, so this stays slow and quiet.
const RECONNECT_DELAY: Duration = Duration::from_secs(30);

/// Messages the worker accepts.
enum Message {
    /// Publish this presence.
    Update(Presence),
    /// Stop publishing and disconnect.
    Clear,
}

/// Handle to the presence worker.
pub struct PresenceHandle {
    tx: Option<Sender<Message>>,
}

impl PresenceHandle {
    /// Starts the worker, or returns an inert handle when presence is disabled.
    ///
    /// Disabled means: no application id configured. The launcher still runs
    /// normally; nothing is published.
    pub fn start() -> Self {
        let app_id = std::env::var(APP_ID_ENV)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_APP_ID.to_string());

        if app_id.trim().is_empty() {
            return Self { tx: None };
        }

        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("discord-presence".into())
            .spawn(move || worker(app_id, rx))
            .ok();
        Self { tx: Some(tx) }
    }

    /// True when presence is configured and a worker is running.
    pub fn is_enabled(&self) -> bool {
        self.tx.is_some()
    }

    /// Publishes `presence`. Never blocks and never fails the caller.
    pub fn update(&self, presence: Presence) {
        self.send(Message::Update(presence));
    }

    /// Clears the presence, e.g. when the user turns it off.
    pub fn clear(&self) {
        self.send(Message::Clear);
    }

    fn send(&self, message: Message) {
        if let Some(tx) = &self.tx {
            // A dead worker means Discord support is gone for this session;
            // that is not worth surfacing.
            let _ = tx.send(message);
        }
    }
}

/// Owns the IPC client for the life of the process.
fn worker(app_id: String, rx: Receiver<Message>) {
    let mut client = DiscordIpcClient::new(&app_id);
    let mut connected = false;
    let mut last_attempt: Option<Instant> = None;
    // Re-sent after a reconnect, so a Discord restart does not blank the status.
    let mut current: Option<Presence> = None;

    while let Ok(message) = rx.recv() {
        match message {
            Message::Update(presence) => current = Some(presence),
            Message::Clear => {
                // `take` both clears and tells us whether anything was live, so
                // a clear with nothing published costs no IPC round trip.
                if current.take().is_some() && connected {
                    let _ = client.clear_activity();
                }
            }
        }

        // Nothing to publish means no reason to hold or open a connection.
        let Some(presence) = current.clone() else {
            continue;
        };

        if !connected {
            // Back off: Discord not running is the normal case, not an error.
            let due = last_attempt.is_none_or(|at| at.elapsed() >= RECONNECT_DELAY);
            if !due {
                continue;
            }
            last_attempt = Some(Instant::now());
            connected = client.connect().is_ok();
            if !connected {
                continue;
            }
        }

        if publish(&mut client, &presence).is_err() {
            // Discord closed or restarted; drop the connection and let the next
            // update reconnect after the backoff.
            connected = false;
            let _ = client.close();
        }
    }

    if connected {
        let _ = client.clear_activity();
        let _ = client.close();
    }
}

/// Sends one activity.
fn publish(client: &mut DiscordIpcClient, presence: &Presence) -> Result<(), DiscordError> {
    client.set_activity(
        activity::Activity::new()
            .details(presence.details.as_str())
            .state(presence.state.as_str())
            .assets(
                activity::Assets::new()
                    .large_image(LARGE_IMAGE_KEY)
                    .large_text(presence.large_text.as_str()),
            )
            .buttons(vec![activity::Button::new(
                "Get the launcher",
                "https://reforgermods.net",
            )]),
    )
}
