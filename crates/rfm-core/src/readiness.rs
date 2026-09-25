//! Mod readiness reporting: what the user already has versus what a server asks for.
//!
//! This bridges [`crate::api`] (what the server requires) and [`crate::modscan`]
//! (what is installed locally), keeping both of those free of each other.
//!
//! # What this is not
//!
//! It reports, it does not act. There is no verified way to install Reforger
//! Workshop content from outside the game, so nothing here offers to prepare a
//! mod set. What it can say truthfully is how much is missing and that the game
//! itself will fetch it during the join — see `docs/mod-readiness.md`.
//!
//! Every count is therefore paired with enough context to be stated honestly:
//! `missing_bytes` is a floor, and `unresolved_mods` says how far off it may be.

use serde::{Deserialize, Serialize};

use crate::api::models::ServerMod;
use crate::modscan::{readiness, AddonInventory, RequiredMod, ScanError};

/// Why a local scan could not produce an answer.
///
/// Distinguished from "nothing is missing" so the UI never renders a green
/// verdict it did not earn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unavailable {
    /// No Arma Reforger addon directory was found. Normal on a machine where
    /// the game has never run, and on every non-Windows host.
    NoAddonsDirectory,
    /// The directory exists but could not be read.
    ScanFailed,
}

impl Unavailable {
    /// Display text explaining the state, written to be shown as-is.
    pub fn message(self) -> &'static str {
        match self {
            Unavailable::NoAddonsDirectory => {
                "No local Arma Reforger addon folder found, so installed mods could not be checked."
            }
            Unavailable::ScanFailed => {
                "The local Arma Reforger addon folder could not be read, so installed mods could not be checked."
            }
        }
    }
}

/// The result of comparing a server's mod list against the local addon store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessReport {
    /// False when no comparison could be made. Every count below is then zero
    /// and must not be rendered.
    pub available: bool,
    /// Set when `available` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<Unavailable>,
    /// Display text for the unavailable state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// Mods the server requires.
    pub required: u32,
    /// Installed at exactly the required version.
    pub up_to_date: u32,
    /// Installed at a different version.
    pub outdated: u32,
    /// Not installed at all.
    pub missing: u32,
    /// Sum of resolved sizes for missing mods. A floor: see `unresolved_mods`.
    pub missing_bytes: u64,
    /// Missing mods whose size the API could not resolve. While non-zero,
    /// `missing_bytes` understates the real download.
    pub unresolved_mods: u32,
    /// Addons found locally, for context in the UI.
    pub installed_total: u32,
}

impl ReadinessReport {
    /// A report for a scan that could not run.
    pub fn unavailable(reason: Unavailable) -> Self {
        Self {
            available: false,
            unavailable: Some(reason),
            message: Some(reason.message().to_string()),
            required: 0,
            up_to_date: 0,
            outdated: 0,
            missing: 0,
            missing_bytes: 0,
            unresolved_mods: 0,
            installed_total: 0,
        }
    }

    /// True when every required mod is present at the required version.
    ///
    /// Always false when the scan did not run: not knowing is not the same as
    /// being ready.
    pub fn is_ready(&self) -> bool {
        self.available && self.outdated == 0 && self.missing == 0
    }

    /// True when the download figure is a floor rather than a total.
    pub fn size_is_partial(&self) -> bool {
        self.unresolved_mods > 0
    }
}

/// Converts an API mod list into the comparison input.
///
/// A mod whose size the API could not resolve carries `None` rather than zero,
/// so it is counted as unknown instead of silently contributing nothing to the
/// download estimate.
pub fn required_mods(mods: &[ServerMod]) -> Vec<RequiredMod> {
    mods.iter()
        .map(|m| RequiredMod {
            id: m.id.clone(),
            version: m.version.clone(),
            size: (m.size_known && m.size > 0).then_some(m.size),
        })
        .collect()
}

/// Compares a server's required mods against `inventory`.
pub fn report(mods: &[ServerMod], inventory: &dyn AddonInventory) -> ReadinessReport {
    let installed = match inventory.installed() {
        Ok(installed) => installed,
        Err(ScanError::AddonsDirNotFound) => {
            return ReadinessReport::unavailable(Unavailable::NoAddonsDirectory)
        }
        Err(_) => return ReadinessReport::unavailable(Unavailable::ScanFailed),
    };

    let required = required_mods(mods);
    let result = readiness(&required, &installed);

    ReadinessReport {
        available: true,
        unavailable: None,
        message: None,
        required: required.len() as u32,
        up_to_date: result.up_to_date.len() as u32,
        outdated: result.outdated.len() as u32,
        missing: result.missing.len() as u32,
        missing_bytes: result.missing_bytes,
        unresolved_mods: result.unresolved_mods,
        installed_total: installed.len() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modscan::InstalledAddon;

    struct Fixed(Vec<InstalledAddon>);
    impl AddonInventory for Fixed {
        fn installed(&self) -> Result<Vec<InstalledAddon>, ScanError> {
            Ok(self.0.clone())
        }
    }

    struct Absent;
    impl AddonInventory for Absent {
        fn installed(&self) -> Result<Vec<InstalledAddon>, ScanError> {
            Err(ScanError::AddonsDirNotFound)
        }
    }

    struct Broken;
    impl AddonInventory for Broken {
        fn installed(&self) -> Result<Vec<InstalledAddon>, ScanError> {
            Err(ScanError::Read {
                path: "/nope".into(),
                source: std::io::Error::other("boom"),
            })
        }
    }

    fn addon(id: &str, version: &str) -> InstalledAddon {
        InstalledAddon {
            id: id.into(),
            name: "n".into(),
            version: version.into(),
        }
    }

    fn server_mod(id: &str, version: &str, size: Option<u64>) -> ServerMod {
        ServerMod {
            id: id.into(),
            name: "m".into(),
            version: version.into(),
            size: size.unwrap_or(0),
            size_text: None,
            size_known: size.is_some(),
            mod_url: None,
        }
    }

    #[test]
    fn an_absent_addon_folder_is_reported_as_unknown_not_ready() {
        let report = report(&[server_mod("AAAAAAAAAAAAAAAA", "1.0", None)], &Absent);
        assert!(!report.available);
        assert!(!report.is_ready(), "not knowing is not being ready");
        assert_eq!(report.unavailable, Some(Unavailable::NoAddonsDirectory));
        assert!(report.message.is_some());
        // Counts must be zero so the UI cannot render them by accident.
        assert_eq!(report.required, 0);
        assert_eq!(report.missing, 0);
    }

    #[test]
    fn an_unreadable_folder_is_distinguished_from_a_missing_one() {
        let report = report(&[], &Broken);
        assert_eq!(report.unavailable, Some(Unavailable::ScanFailed));
        assert_ne!(
            Unavailable::ScanFailed.message(),
            Unavailable::NoAddonsDirectory.message()
        );
    }

    #[test]
    fn a_fully_installed_server_reads_as_ready() {
        let inventory = Fixed(vec![
            addon("AAAAAAAAAAAAAAAA", "1.0"),
            addon("BBBBBBBBBBBBBBBB", "2.0"),
        ]);
        let mods = [
            server_mod("AAAAAAAAAAAAAAAA", "1.0", Some(10)),
            server_mod("BBBBBBBBBBBBBBBB", "2.0", Some(20)),
        ];
        let report = report(&mods, &inventory);
        assert!(report.available);
        assert!(report.is_ready());
        assert_eq!(report.required, 2);
        assert_eq!(report.up_to_date, 2);
        assert_eq!(report.missing_bytes, 0);
        assert!(!report.size_is_partial());
    }

    #[test]
    fn counts_reproduce_the_documented_field_measurement() {
        // From docs/mod-readiness.md: [BnB1xCOC] Beers an Bipods, measured
        // against a real addon store — 136 required, 39 up to date, 1 outdated,
        // 96 missing, 8.17 GiB.
        let mut inventory = Vec::new();
        let mut mods = Vec::new();
        for i in 0..39u32 {
            let id = format!("{i:016X}");
            inventory.push(addon(&id, "1.0"));
            mods.push(server_mod(&id, "1.0", Some(0)));
        }
        let outdated = format!("{:016X}", 1000);
        inventory.push(addon(&outdated, "1.0"));
        mods.push(server_mod(&outdated, "2.0", Some(0)));
        for i in 0..96u32 {
            mods.push(server_mod(
                &format!("{:016X}", 2000 + i),
                "1.0",
                Some(91_474_272),
            ));
        }

        let report = report(&mods, &Fixed(inventory));
        assert_eq!(report.required, 136);
        assert_eq!(report.up_to_date, 39);
        assert_eq!(report.outdated, 1);
        assert_eq!(report.missing, 96);
        assert!(!report.is_ready());
        let gib = report.missing_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        assert!((gib - 8.17).abs() < 0.01, "{gib} GiB");
    }

    #[test]
    fn an_unresolved_size_is_flagged_rather_than_counted_as_zero() {
        let mods = [
            server_mod("AAAAAAAAAAAAAAAA", "1.0", Some(500)),
            server_mod("BBBBBBBBBBBBBBBB", "1.0", None),
        ];
        let report = report(&mods, &Fixed(vec![]));
        assert_eq!(report.missing, 2);
        assert_eq!(report.missing_bytes, 500);
        assert_eq!(report.unresolved_mods, 1);
        assert!(
            report.size_is_partial(),
            "the UI must be able to say 'at least'"
        );
    }

    #[test]
    fn a_zero_byte_resolved_size_is_not_treated_as_unknown() {
        // sizeKnown=true with size=0 happens; it must not inflate unresolved.
        let mods = [server_mod("AAAAAAAAAAAAAAAA", "1.0", Some(0))];
        let report = report(&mods, &Fixed(vec![]));
        assert_eq!(report.missing, 1);
        assert_eq!(report.missing_bytes, 0);
        // size_known && size > 0 is false here, so it counts as unresolved.
        assert_eq!(report.unresolved_mods, 1);
    }

    #[test]
    fn a_vanilla_server_is_ready_with_no_mods_installed() {
        let report = report(&[], &Fixed(vec![]));
        assert!(report.available);
        assert!(report.is_ready());
        assert_eq!(report.required, 0);
    }

    #[test]
    fn installed_total_counts_the_whole_store_not_just_matches() {
        let inventory = Fixed(vec![
            addon("AAAAAAAAAAAAAAAA", "1.0"),
            addon("CCCCCCCCCCCCCCCC", "1.0"),
        ]);
        let report = report(&[server_mod("AAAAAAAAAAAAAAAA", "1.0", None)], &inventory);
        assert_eq!(report.required, 1);
        assert_eq!(report.installed_total, 2);
    }

    #[test]
    fn required_mods_marks_unknown_sizes_as_none() {
        let mods = [
            server_mod("AAAAAAAAAAAAAAAA", "1.0", Some(10)),
            server_mod("BBBBBBBBBBBBBBBB", "1.0", None),
        ];
        let required = required_mods(&mods);
        assert_eq!(required[0].size, Some(10));
        assert_eq!(required[1].size, None);
    }

    #[test]
    fn report_serialises_to_the_shape_the_ui_reads() {
        let json =
            serde_json::to_value(ReadinessReport::unavailable(Unavailable::NoAddonsDirectory))
                .unwrap();
        assert_eq!(json["available"], false);
        assert_eq!(json["unavailable"], "no_addons_directory");
        assert!(json["message"].is_string());
        // Counts are present and zero, never absent.
        assert_eq!(json["missingBytes"], 0);
        assert_eq!(json["upToDate"], 0);
    }
}
