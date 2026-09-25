//! Direct-join URI construction for Arma Reforger.
//!
//! The join payload is a base64 encoding of a small JSON object naming the
//! lobby room:
//!
//! ```text
//! {"id":"<server-uuid>","inviteToken":""}
//!   -> UTF-8 bytes
//!   -> standard base64
//!   -> trailing '=' padding removed
//! ```
//!
//! # How the payload reaches the game
//!
//! The payload is delivered as a Steam launch argument:
//!
//! ```text
//! steam://run/1874880//<payload>/
//!                     ^^
//! ```
//!
//! The **empty path segment** is what matters. `steam://run/<appid>//<args>/`
//! is Steam's documented form for passing launch arguments; with a single
//! slash, Steam can start the game without forwarding the payload, leaving the
//! player on the main menu.
//!
//! Status: this is the current best understanding and is **not yet confirmed
//! against a live client**. The single-slash form was observed to launch the
//! game without joining; the double-slash form is the likely fix.
//!
//! # The `ArmaReforger:` protocol, an alternative
//!
//! The game also registers its own URL protocol. Its Steam install script
//! (`steamapps/common/Arma Reforger/installscript.vdf`) writes:
//!
//! ```text
//! HKCU\SOFTWARE\Classes\ArmaReforger
//!   (Default)    = "URL:Arma Reforger Protocol"
//!   URL Protocol = ""
//! HKCU\SOFTWARE\Classes\ArmaReforger\shell\open\command
//!   (Default)    = "<dir>\ArmaReforger_BE.exe" -exe ArmaReforgerSteam.exe
//!                  "-addonsDir=<dir>\addons" -uri="%1"
//! ```
//!
//! [`reforger_join_uri`] builds that form. It is kept, tested and unused by
//! default: it is a second candidate delivery route if the Steam form turns out
//! not to join, and it does not depend on how Steam forwards arguments.
//!
//! # Launch state
//!
//! Either route only carries the payload when the game actually starts. If Arma
//! Reforger is already running, Steam may simply focus the existing process and
//! drop the arguments, so a join must be tested from a fully closed game.
//!
//! Everything in this module is pure. It never spawns a shell and never
//! interpolates API-supplied text: the only value that reaches a URI is a
//! [`ServerId`], which cannot be constructed without passing UUID validation.

use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine as _;
use serde::Serialize;

/// Steam App ID for Arma Reforger.
pub const ARMA_REFORGER_APP_ID: u32 = 1874880;

/// URL protocol Arma Reforger registers for join links.
///
/// Matches the `HKCU\SOFTWARE\Classes\ArmaReforger` key written by the game's
/// Steam install script. URI schemes are case-insensitive; this spelling
/// mirrors the registry key.
pub const REFORGER_URI_SCHEME: &str = "ArmaReforger";

/// Errors produced while validating a server identifier.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LaunchError {
    /// The identifier was not a canonical 8-4-4-4-12 lowercase-hex UUID.
    #[error("not a valid Arma Reforger server id (expected a UUID, got {got:?})")]
    InvalidServerId { got: String },
}

/// A validated Arma Reforger server identifier.
///
/// Two shapes occur in the API, and both must be joinable:
///
/// - **Community servers** use a canonical UUID, e.g.
///   `73e8fb7f-8789-4d88-8aa8-ea69b9aa092f`.
/// - **Official servers** use a provider id, e.g. `nitrado_17516010`. Sampled
///   across 600 servers, every non-UUID id was official and every official id
///   was `nitrado_` followed by digits.
///
/// The only way to obtain one is [`ServerId::parse`], so any `ServerId` in the
/// program contains nothing but lowercase alphanumerics, `-` and `_`. This is
/// the security boundary for launching: untrusted API text cannot become a URI.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct ServerId(String);

impl ServerId {
    /// Parses a canonical UUID. Case is normalised to lowercase; surrounding
    /// whitespace is rejected rather than trimmed, because a stray space in an
    /// id means the caller mishandled the value somewhere upstream.
    pub fn parse(raw: &str) -> Result<Self, LaunchError> {
        if is_canonical_uuid(raw) || is_provider_id(raw) {
            Ok(Self(raw.to_ascii_lowercase()))
        } else {
            Err(LaunchError::InvalidServerId {
                got: raw.to_owned(),
            })
        }
    }

    /// True for an official (provider-hosted) server.
    pub fn is_official_form(&self) -> bool {
        is_provider_id(&self.0)
    }

    /// The validated identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ServerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Returns true for exactly `8-4-4-4-12` ASCII-hex groups and nothing else.
///
/// Hand-written rather than pulled from a UUID crate so the accepted shape is
/// visible here: no braces, no `urn:` prefix, no internal whitespace.
fn is_canonical_uuid(s: &str) -> bool {
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];
    let mut parts = s.split('-');
    for len in GROUPS {
        match parts.next() {
            Some(p) if p.len() == len && p.bytes().all(|b| b.is_ascii_hexdigit()) => {}
            _ => return false,
        }
    }
    parts.next().is_none()
}

/// Returns true for `<provider>_<digits>`, the official-server id shape.
///
/// Deliberately narrow: lowercase ASCII letters, one underscore, then digits.
/// Every character it admits is URI-safe, so a validated id never needs
/// escaping. Written generically rather than hardcoding `nitrado` so a second
/// provider does not silently become unjoinable.
fn is_provider_id(s: &str) -> bool {
    let Some((provider, number)) = s.split_once('_') else {
        return false;
    };
    !provider.is_empty()
        && provider.len() <= 16
        && provider.bytes().all(|b| b.is_ascii_lowercase())
        && !number.is_empty()
        && number.len() <= 24
        && number.bytes().all(|b| b.is_ascii_digit())
}

/// The JSON body Arma Reforger expects. Field order is the declaration order,
/// which is what the verified payload uses.
#[derive(Serialize)]
struct JoinPayload<'a> {
    id: &'a str,
    #[serde(rename = "inviteToken")]
    invite_token: &'a str,
}

/// Serialises the join payload for a server.
///
/// Exposed separately from [`steam_join_uri`] so the exact bytes that get
/// encoded are directly assertable in tests.
pub fn join_payload_json(server: &ServerId) -> String {
    let payload = JoinPayload {
        id: server.as_str(),
        invite_token: "",
    };
    // A struct of two string fields cannot fail to serialise.
    serde_json::to_string(&payload).expect("join payload is always serialisable")
}

/// Base64-encodes the join payload with trailing `=` padding removed.
pub fn join_payload_base64(server: &ServerId) -> String {
    STANDARD_NO_PAD.encode(join_payload_json(server).as_bytes())
}

/// Builds the `ArmaReforger://` URI for the game's own protocol handler.
///
/// Not used by default; see the module docs. Retained as a second candidate
/// delivery route, since it reaches the launcher as `-uri=` without depending
/// on how Steam forwards arguments.
pub fn reforger_join_uri(server: &ServerId) -> String {
    format!("{REFORGER_URI_SCHEME}://{}", join_payload_base64(server))
}

/// Convenience: validate a raw identifier and build its join URI in one step.
pub fn reforger_join_uri_for(raw: &str) -> Result<String, LaunchError> {
    Ok(reforger_join_uri(&ServerId::parse(raw)?))
}

/// Builds the `steam://run/...` URI that joins `server` directly.
///
/// Note the doubled slash: `steam://run/<appid>//<args>/`. The empty path
/// segment marks what follows as a launch argument. Dropping it produces a URI
/// that starts the game without forwarding the payload.
///
/// Hand this to the platform URI opener (`ShellExecuteW` on Windows via
/// `tauri-plugin-opener`). Never pass it to a shell.
pub fn steam_join_uri(server: &ServerId) -> String {
    format!(
        "steam://run/{}//{}/",
        ARMA_REFORGER_APP_ID,
        join_payload_base64(server)
    )
}

/// Convenience: validate a raw identifier and build its join URI in one step.
pub fn steam_join_uri_for(raw: &str) -> Result<String, LaunchError> {
    Ok(steam_join_uri(&ServerId::parse(raw)?))
}

/// Builds the `steam://run/<appid>` URI that starts Arma Reforger with no
/// server argument, for "just launch the game".
pub fn steam_launch_uri() -> String {
    format!("steam://run/{ARMA_REFORGER_APP_ID}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identifier and payload from the verified PowerShell experiment.
    const SAMPLE_ID: &str = "73e8fb7f-8789-4d88-8aa8-ea69b9aa092f";
    const SAMPLE_B64: &str =
        "eyJpZCI6IjczZThmYjdmLTg3ODktNGQ4OC04YWE4LWVhNjliOWFhMDkyZiIsImludml0ZVRva2VuIjoiIn0";

    #[test]
    fn payload_json_matches_verified_shape() {
        let id = ServerId::parse(SAMPLE_ID).unwrap();
        assert_eq!(
            join_payload_json(&id),
            r#"{"id":"73e8fb7f-8789-4d88-8aa8-ea69b9aa092f","inviteToken":""}"#
        );
    }

    #[test]
    fn payload_base64_matches_verified_vector() {
        let id = ServerId::parse(SAMPLE_ID).unwrap();
        assert_eq!(join_payload_base64(&id), SAMPLE_B64);
    }

    #[test]
    fn base64_padding_is_removed() {
        let id = ServerId::parse(SAMPLE_ID).unwrap();
        let encoded = join_payload_base64(&id);
        assert!(
            !encoded.ends_with('='),
            "padding must be stripped: {encoded}"
        );
        // The 62-byte payload would normally encode to 84 chars with one '='.
        assert_eq!(join_payload_json(&id).len(), 62);
        assert_eq!(encoded.len(), 83);
    }

    #[test]
    fn padding_is_removed_for_every_residue_class() {
        // UUID payloads are a fixed length, so exercise the encoder directly to
        // cover all three base64 remainder cases (0, 1 and 2 leftover bytes).
        for input in ["abc", "abcd", "abcde", ""] {
            let encoded = STANDARD_NO_PAD.encode(input.as_bytes());
            assert!(!encoded.contains('='), "{input:?} -> {encoded}");
        }
    }

    #[test]
    fn join_uri_has_the_empty_segment_that_marks_a_launch_argument() {
        // This is the entire fix and it is one character. Without the empty
        // segment Steam can start the game without forwarding the payload,
        // which looks like a successful launch that never joins.
        let id = ServerId::parse(SAMPLE_ID).unwrap();
        let uri = steam_join_uri(&id);
        assert_eq!(uri, format!("steam://run/1874880//{SAMPLE_B64}/"));
        assert!(
            uri.contains(&format!("{ARMA_REFORGER_APP_ID}//")),
            "app id must be followed by an empty path segment: {uri}"
        );
        assert!(
            !uri.contains(&format!("{ARMA_REFORGER_APP_ID}/{SAMPLE_B64}")),
            "single-slash form regressed: {uri}"
        );
    }

    #[test]
    fn join_uri_matches_the_reference_implementation() {
        // Mirrors the JS reference: JSON.stringify({id, inviteToken}) in that
        // field order, standard base64, trailing '=' stripped, wrapped as
        // steam://run/1874880//<payload>/.
        let id = ServerId::parse(SAMPLE_ID).unwrap();
        let payload = join_payload_base64(&id);
        assert!(!payload.contains('='));
        assert_eq!(
            steam_join_uri(&id),
            format!("steam://run/{ARMA_REFORGER_APP_ID}//{payload}/")
        );
    }

    #[test]
    fn plain_launch_uri_keeps_a_single_segment() {
        // No payload means no argument marker; this must not gain the empty
        // segment along with the join URI.
        assert_eq!(steam_launch_uri(), "steam://run/1874880");
    }

    #[test]
    fn the_protocol_alternative_carries_an_identical_payload() {
        let id = ServerId::parse(SAMPLE_ID).unwrap();
        let payload = join_payload_base64(&id);
        assert!(steam_join_uri(&id).contains(&payload));
        assert!(reforger_join_uri(&id).ends_with(&payload));
    }

    #[test]
    fn join_uri_uses_the_games_own_protocol() {
        // The scheme the game registers; Steam's run URL drops the payload.
        let id = ServerId::parse(SAMPLE_ID).unwrap();
        assert_eq!(
            reforger_join_uri(&id),
            format!("ArmaReforger://{SAMPLE_B64}")
        );
    }

    #[test]
    fn join_uri_carries_the_same_payload_as_the_steam_form() {
        let id = ServerId::parse(SAMPLE_ID).unwrap();
        let payload = join_payload_base64(&id);
        assert!(reforger_join_uri(&id).ends_with(&payload));
        assert!(steam_join_uri(&id).contains(&payload));
    }

    #[test]
    fn join_uri_has_no_trailing_slash() {
        // The whole URI is forwarded verbatim as -uri=; a stray separator would
        // become part of the payload the game parses.
        let uri = reforger_join_uri_for(SAMPLE_ID).unwrap();
        assert!(!uri.ends_with('/'), "{uri}");
        assert_eq!(uri.matches("//").count(), 1);
    }

    #[test]
    fn steam_join_uri_is_stable_for_a_given_server() {
        let id = ServerId::parse(SAMPLE_ID).unwrap();
        assert_eq!(steam_join_uri(&id), steam_join_uri_for(SAMPLE_ID).unwrap());
    }

    #[test]
    fn join_uri_has_trailing_slash_and_app_id() {
        let uri = steam_join_uri_for(SAMPLE_ID).unwrap();
        assert!(uri.starts_with("steam://run/1874880//"));
        assert!(uri.ends_with('/'));
    }

    #[test]
    fn plain_launch_uri_carries_no_payload() {
        assert_eq!(steam_launch_uri(), "steam://run/1874880");
    }

    #[test]
    fn uppercase_ids_are_normalised() {
        let id = ServerId::parse("73E8FB7F-8789-4D88-8AA8-EA69B9AA092F").unwrap();
        assert_eq!(id.as_str(), SAMPLE_ID);
        assert_eq!(
            reforger_join_uri(&id),
            reforger_join_uri_for(SAMPLE_ID).unwrap()
        );
    }

    /// A real official server id, from `/v2/servers?official=true`.
    const OFFICIAL_ID: &str = "nitrado_17516010";

    #[test]
    fn official_server_ids_are_accepted() {
        // 364 official servers use this shape; rejecting it made them all
        // unjoinable.
        let id = ServerId::parse(OFFICIAL_ID).unwrap();
        assert_eq!(id.as_str(), OFFICIAL_ID);
        assert!(id.is_official_form());
        assert!(!ServerId::parse(SAMPLE_ID).unwrap().is_official_form());
    }

    #[test]
    fn official_ids_join_through_the_same_payload() {
        // The payload shape does not change: only the id inside it does.
        let id = ServerId::parse(OFFICIAL_ID).unwrap();
        assert_eq!(
            join_payload_json(&id),
            r#"{"id":"nitrado_17516010","inviteToken":""}"#
        );
        assert_eq!(
            steam_join_uri(&id),
            "steam://run/1874880//eyJpZCI6Im5pdHJhZG9fMTc1MTYwMTAiLCJpbnZpdGVUb2tlbiI6IiJ9/"
        );
    }

    #[test]
    fn provider_ids_stay_uri_safe() {
        // Everything the provider shape admits can sit in a URI unescaped, so
        // no id ever needs encoding on the way out.
        for raw in ["nitrado_17516010", "a_0", "somehost_123456789012"] {
            let id = ServerId::parse(raw).unwrap();
            assert!(id
                .as_str()
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'));
        }
    }

    #[test]
    fn provider_shaped_lookalikes_are_rejected() {
        for raw in [
            "_123",                  // no provider
            "nitrado_",              // no number
            "nitrado_17516010x",     // trailing junk
            "Nitrado_17516010",      // uppercase provider
            "nitrado-17516010",      // wrong separator
            "nitrado_175_160",       // second underscore
            "nitrado_17516010/evil", // path injection
            "averyveryverylongprovidername_1",
            "nitrado_1234567890123456789012345",
        ] {
            assert!(ServerId::parse(raw).is_err(), "should be rejected: {raw:?}");
        }
    }

    #[test]
    fn invalid_ids_are_rejected() {
        let rejected = [
            "",
            "not-a-uuid",
            // Wrong group lengths.
            "73e8fb7f-8789-4d88-8aa8-ea69b9aa092",
            "73e8fb7f-8789-4d88-8aa8-ea69b9aa092ff",
            "73e8fb7-88789-4d88-8aa8-ea69b9aa092f",
            // Non-hex characters.
            "73e8fb7g-8789-4d88-8aa8-ea69b9aa092f",
            // Extra group.
            "73e8fb7f-8789-4d88-8aa8-ea69b9aa092f-0000",
            // Braced / URN forms are not what the API returns.
            "{73e8fb7f-8789-4d88-8aa8-ea69b9aa092f}",
            "urn:uuid:73e8fb7f-8789-4d88-8aa8-ea69b9aa092f",
            // Whitespace must not be silently trimmed.
            " 73e8fb7f-8789-4d88-8aa8-ea69b9aa092f",
            "73e8fb7f-8789-4d88-8aa8-ea69b9aa092f ",
            // Unpadded / no separators.
            "73e8fb7f87894d888aa8ea69b9aa092f",
        ];
        for raw in rejected {
            assert!(
                ServerId::parse(raw).is_err(),
                "should have been rejected: {raw:?}"
            );
            assert!(steam_join_uri_for(raw).is_err(), "{raw:?}");
            assert!(reforger_join_uri_for(raw).is_err(), "{raw:?}");
        }
    }

    #[test]
    fn injection_attempts_never_reach_a_uri() {
        // These are the shapes that would matter if an id were ever pasted into
        // a command line or a URI. Validation rejects them before that point.
        let hostile = [
            "73e8fb7f-8789-4d88-8aa8-ea69b9aa092f/../../evil",
            "73e8fb7f-8789-4d88-8aa8-ea69b9aa092f\"; calc.exe",
            "73e8fb7f-8789-4d88-8aa8-ea69b9aa092f&&shutdown",
            "$(whoami)-8789-4d88-8aa8-ea69b9aa092f",
            "73e8fb7f-8789-4d88-8aa8-ea69b9aa09\n\r",
        ];
        for raw in hostile {
            assert_eq!(
                ServerId::parse(raw),
                Err(LaunchError::InvalidServerId {
                    got: raw.to_owned()
                })
            );
        }
    }

    #[test]
    fn generated_uris_contain_only_url_safe_payload_characters() {
        // Standard base64 can emit '+' and '/', which are legal in this Steam
        // argument position; assert the payload never contains anything beyond
        // that set so the URI is always well-formed.
        let id = ServerId::parse(SAMPLE_ID).unwrap();
        let payload = join_payload_base64(&id);
        assert!(payload
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/'));
    }
}
