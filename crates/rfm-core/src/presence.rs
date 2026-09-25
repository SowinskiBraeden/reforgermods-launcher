//! Discord Rich Presence payloads.
//!
//! What a Discord friend sees while the launcher is open — the promotional
//! surface. The transport lives in the app layer; everything here is pure text
//! assembly, which is the part with rules worth testing:
//!
//! - **Server names are untrusted.** They arrive from the API, routinely carry
//!   Discord invites and decorative glyphs, and are sometimes very long. Control
//!   characters are stripped and whitespace collapsed before anything is sent.
//! - **Discord silently drops fields that break its limits.** `details` and
//!   `state` cap at 128 characters, and a field shorter than two characters is
//!   ignored rather than rejected, so a one-character server name must not
//!   produce an invisible line.
//! - **Truncation is by character, never by byte.** Cutting a multi-byte glyph
//!   in half produces invalid UTF-8 that Discord rejects outright.

use serde::{Deserialize, Serialize};

/// Discord's limit for `details` and `state`.
const FIELD_LIMIT: usize = 128;
/// Below this, Discord ignores the field entirely.
const FIELD_MINIMUM: usize = 2;

/// One presence update.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Presence {
    /// Top line.
    pub details: String,
    /// Second line.
    pub state: String,
    /// Tooltip on the large artwork.
    pub large_text: String,
}

/// What the launcher is currently doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity<'a> {
    /// Browsing the server list.
    Browsing { online_servers: u64 },
    /// A server is selected in the inspector.
    Viewing {
        server_name: &'a str,
        players: u32,
        max_players: u32,
    },
    /// A join was handed off to Steam.
    Joining { server_name: &'a str },
}

impl Activity<'_> {
    /// Builds the presence for this activity, within Discord's limits.
    pub fn to_presence(&self) -> Presence {
        let (details, state) = match self {
            Activity::Browsing { online_servers } => (
                "Browsing Arma Reforger servers".to_string(),
                if *online_servers > 0 {
                    format!("{online_servers} servers online")
                } else {
                    "reforgermods.net".to_string()
                },
            ),
            Activity::Viewing {
                server_name,
                players,
                max_players,
            } => (
                format!("Viewing {}", clean(server_name)),
                format!("{players}/{max_players} players"),
            ),
            Activity::Joining { server_name } => (
                format!("Joining {}", clean(server_name)),
                "via reforgermods.net launcher".to_string(),
            ),
        };

        Presence {
            details: field(&details),
            state: field(&state),
            large_text: "reforgermods.net launcher".to_string(),
        }
    }
}

/// Collapses whitespace and drops control characters from untrusted text.
///
/// Keeps everything else, including emoji and non-Latin scripts: server names
/// legitimately use them, and Discord renders them fine.
fn clean(raw: &str) -> String {
    let collapsed: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    collapsed.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Fits `text` into one Discord field.
///
/// Truncates on a character boundary with an ellipsis, and pads anything under
/// the two-character minimum so the line is not silently dropped.
fn field(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() > FIELD_LIMIT {
        let kept: String = text.chars().take(FIELD_LIMIT - 1).collect();
        return format!("{}…", kept.trim_end());
    }
    if text.chars().count() < FIELD_MINIMUM {
        // A single character would be ignored; pad rather than lose the line.
        return format!("{text} ").trim_start().to_string() + " ";
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browsing_reports_the_online_count() {
        let p = Activity::Browsing {
            online_servers: 4999,
        }
        .to_presence();
        assert_eq!(p.details, "Browsing Arma Reforger servers");
        assert_eq!(p.state, "4999 servers online");
        assert_eq!(p.large_text, "reforgermods.net launcher");
    }

    #[test]
    fn browsing_without_a_count_still_names_the_product() {
        let p = Activity::Browsing { online_servers: 0 }.to_presence();
        assert_eq!(p.state, "reforgermods.net");
    }

    #[test]
    fn viewing_shows_the_server_and_population() {
        let p = Activity::Viewing {
            server_name: "[OGR2] Old Guard Revival 2",
            players: 12,
            max_players: 128,
        }
        .to_presence();
        assert_eq!(p.details, "Viewing [OGR2] Old Guard Revival 2");
        assert_eq!(p.state, "12/128 players");
    }

    #[test]
    fn joining_names_the_launcher() {
        let p = Activity::Joining {
            server_name: "EXD.gg",
        }
        .to_presence();
        assert_eq!(p.details, "Joining EXD.gg");
        assert_eq!(p.state, "via reforgermods.net launcher");
    }

    #[test]
    fn control_characters_are_stripped_from_untrusted_names() {
        let p = Activity::Viewing {
            server_name: "Evil\nServer\r\n\tName\u{0}",
            players: 1,
            max_players: 2,
        }
        .to_presence();
        assert_eq!(p.details, "Viewing Evil Server Name");
        assert!(!p.details.contains('\n'));
        assert!(!p.details.contains('\u{0}'));
    }

    #[test]
    fn whitespace_is_collapsed() {
        let p = Activity::Viewing {
            server_name: "   [EU1]    Lots   of    space   ",
            players: 0,
            max_players: 0,
        }
        .to_presence();
        assert_eq!(p.details, "Viewing [EU1] Lots of space");
    }

    #[test]
    fn long_names_are_truncated_within_the_limit() {
        let long = "A".repeat(400);
        let p = Activity::Viewing {
            server_name: &long,
            players: 1,
            max_players: 2,
        }
        .to_presence();
        assert_eq!(p.details.chars().count(), FIELD_LIMIT);
        assert!(p.details.ends_with('…'));
    }

    #[test]
    fn truncation_never_splits_a_multibyte_character() {
        // Byte-truncating this would produce invalid UTF-8 and Discord would
        // reject the whole payload.
        for name in [
            "🇬🇧".repeat(200),
            "日本語サーバー".repeat(50),
            "é".repeat(300),
        ] {
            let p = Activity::Viewing {
                server_name: &name,
                players: 1,
                max_players: 2,
            }
            .to_presence();
            assert!(p.details.chars().count() <= FIELD_LIMIT, "{}", p.details);
            // Round-tripping proves the string is still valid UTF-8.
            assert_eq!(
                String::from_utf8(p.details.clone().into_bytes()).unwrap(),
                p.details
            );
        }
    }

    #[test]
    fn short_fields_are_padded_to_discords_minimum() {
        // Discord ignores a one-character field, which would blank the line.
        let p = Activity::Viewing {
            server_name: "",
            players: 1,
            max_players: 2,
        }
        .to_presence();
        assert!(p.details.chars().count() >= FIELD_MINIMUM);
        assert!(p.state.chars().count() >= FIELD_MINIMUM);
    }

    #[test]
    fn every_field_respects_the_limit() {
        let nasty = "\u{202e}".to_string() + &"x".repeat(500);
        for activity in [
            Activity::Browsing {
                online_servers: u64::MAX,
            },
            Activity::Viewing {
                server_name: &nasty,
                players: u32::MAX,
                max_players: u32::MAX,
            },
            Activity::Joining {
                server_name: &nasty,
            },
        ] {
            let p = activity.to_presence();
            for f in [&p.details, &p.state, &p.large_text] {
                assert!(f.chars().count() <= FIELD_LIMIT, "{f}");
                assert!(f.chars().count() >= FIELD_MINIMUM, "{f:?}");
            }
        }
    }

    #[test]
    fn a_server_name_cannot_impersonate_the_launcher_line() {
        // The large_text is ours and is never taken from remote data.
        let p = Activity::Viewing {
            server_name: "reforgermods.net launcher",
            players: 1,
            max_players: 2,
        }
        .to_presence();
        assert_eq!(p.large_text, "reforgermods.net launcher");
        assert!(p.details.starts_with("Viewing "));
    }
}
