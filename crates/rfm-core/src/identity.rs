//! Stable, anonymous installation identity.
//!
//! The API needs to tell "50 000 requests from 5 000 installs" from "50 000
//! requests from one". A value generated on first run answers that, but it
//! resets whenever the launcher is reinstalled, so one person reinstalling
//! three times counts as three installs.
//!
//! This derives the identifier from a machine characteristic instead, so it
//! survives uninstall and reinstall.
//!
//! # What is sent, and what is not
//!
//! The machine value **never leaves the process**. What is sent is
//!
//! ```text
//! SHA-256(APPLICATION_SALT || machine value) -> first 16 bytes -> UUID
//! ```
//!
//! so the API receives an opaque identifier it cannot reverse into a machine
//! id, and the same machine running a different application derives a different
//! value. The wire format stays a canonical UUID, so the API contract in
//! `docs/client-identification.md` is unchanged.
//!
//! # Why not IP
//!
//! An address is the wrong input: it changes with DHCP leases, VPNs, tethering
//! and moving between networks, so it would both split one install into many
//! and merge everyone behind a shared NAT into one. It is also the one piece of
//! genuinely personal data in the request, and the API already sees it.
//!
//! # Sources
//!
//! | Platform | Value |
//! | --- | --- |
//! | Windows | `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid` |
//! | Linux | `/etc/machine-id`, else `/var/lib/dbus/machine-id` |
//! | Other / unreadable | falls back to a persisted random value |
//!
//! All of these are per-OS-installation, not per-user and not per-device-serial,
//! and they change when the OS is reinstalled — which is the intended
//! granularity: a new OS is a new install.

use sha2::{Digest, Sha256};

/// Domain separator. Mixed in so the same machine yields a different identifier
/// for a different application, and so the digest is not a bare hash of a value
/// that other software also knows.
const APPLICATION_SALT: &str = "reforgermods.net-launcher/install-id/v1";

/// The identifier to send, preferring a machine-derived value.
///
/// `fallback` is the persisted random value used when no machine source is
/// readable — a sandboxed or unusual environment still gets a working, if
/// reinstall-unstable, identifier rather than none.
pub fn stable_install_id(fallback: &str) -> String {
    match machine_value() {
        Some(value) => derive_uuid(&value),
        None => fallback.to_string(),
    }
}

/// True when this build could read a machine source, so the identifier survives
/// a reinstall. Surfaced only in debug mode.
pub fn is_machine_derived() -> bool {
    machine_value().is_some()
}

/// Hashes `material` into a canonical UUID.
///
/// The result carries UUID version 8 (custom) and the RFC 4122 variant, which
/// is the correct label for a value derived this way — it is not random (v4) and
/// not an RFC-specified name hash (v3/v5).
fn derive_uuid(material: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(APPLICATION_SALT.as_bytes());
    hasher.update([0u8]); // Separator, so salt and value cannot run together.
    hasher.update(material.trim().to_ascii_lowercase().as_bytes());
    let digest = hasher.finalize();

    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80; // version 8
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // RFC 4122 variant

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

/// Reads this machine's identifier, if one is available.
#[cfg(windows)]
fn machine_value() -> Option<String> {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
    use winreg::RegKey;

    // Explicitly the 64-bit view: MachineGuid lives there, and a 32-bit process
    // would otherwise be redirected to an empty Wow6432Node.
    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Cryptography",
            KEY_READ | KEY_WOW64_64KEY,
        )
        .ok()?;
    let guid: String = key.get_value("MachineGuid").ok()?;
    usable(guid)
}

/// Reads this machine's identifier, if one is available.
#[cfg(not(windows))]
fn machine_value() -> Option<String> {
    ["/etc/machine-id", "/var/lib/dbus/machine-id"]
        .iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
        .and_then(usable)
}

/// Rejects blank or obviously placeholder values.
///
/// An all-zero or empty machine id would make every affected machine share one
/// identifier, which is worse than falling back to a random one.
fn usable(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    if trimmed.len() < 8 {
        return None;
    }
    if trimmed.chars().all(|c| c == '0' || c == '-') {
        return None;
    }
    Some(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::ServerId;

    #[test]
    fn derived_ids_are_canonical_uuids() {
        // The API header validator accepts only this shape, so a derived value
        // that failed it would be silently dropped.
        let id = derive_uuid("4c4c4544-0037-5910-8051-b8c04f325632");
        assert!(ServerId::parse(&id).is_ok(), "{id}");
        assert_eq!(id.len(), 36);
    }

    #[test]
    fn derivation_is_deterministic() {
        let a = derive_uuid("machine-abc");
        let b = derive_uuid("machine-abc");
        assert_eq!(a, b, "the same machine must reinstall to the same id");
    }

    #[test]
    fn different_machines_derive_different_ids() {
        assert_ne!(derive_uuid("machine-abc"), derive_uuid("machine-abd"));
    }

    #[test]
    fn input_is_normalised_for_case_and_whitespace() {
        // Windows returns the GUID in varying case depending on how it is read.
        let canonical = derive_uuid("4c4c4544-0037-5910-8051-b8c04f325632");
        assert_eq!(
            derive_uuid("4C4C4544-0037-5910-8051-B8C04F325632"),
            canonical
        );
        assert_eq!(
            derive_uuid("  4c4c4544-0037-5910-8051-b8c04f325632\n"),
            canonical
        );
    }

    #[test]
    fn the_machine_value_is_not_recoverable_from_the_id() {
        // Not a proof, but it pins the intent: the identifier must never be the
        // machine value, or a prefix or suffix of it.
        let machine = "4c4c4544-0037-5910-8051-b8c04f325632";
        let id = derive_uuid(machine);
        assert_ne!(id, machine);
        assert!(!id.contains(&machine[..8]));
        assert!(!machine.contains(&id[..8]));
    }

    #[test]
    fn version_and_variant_are_set() {
        let id = derive_uuid("anything");
        let bytes: Vec<u8> = id
            .replace('-', "")
            .as_bytes()
            .chunks(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        assert_eq!(bytes[6] >> 4, 8, "version 8, a custom derivation");
        assert_eq!(bytes[8] >> 6, 0b10, "RFC 4122 variant");
    }

    #[test]
    fn the_salt_separates_this_application() {
        // Two applications on one machine must not share an identifier. Mirrors
        // derive_uuid with a different salt.
        let mut hasher = Sha256::new();
        hasher.update(b"some-other-application");
        hasher.update([0u8]);
        hasher.update(b"machine-abc");
        let other = hasher.finalize();
        assert_ne!(
            derive_uuid("machine-abc")
                .replace('-', "")
                .chars()
                .take(8)
                .collect::<String>(),
            other[..4]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }

    #[test]
    fn unusable_machine_values_are_rejected() {
        // These would collapse many machines onto one identifier.
        for value in [
            "",
            "   ",
            "0",
            "0000",
            "00000000-0000-0000-0000-000000000000",
            "-----",
        ] {
            assert!(usable(value.to_string()).is_none(), "{value:?}");
        }
        assert!(usable("4c4c4544-0037-5910-8051-b8c04f325632".into()).is_some());
    }

    #[test]
    fn the_fallback_is_used_when_no_machine_value_exists() {
        // Cannot force machine_value() to fail here, so this pins the contract
        // that a fallback is returned verbatim rather than hashed.
        let fallback = "8f14e45f-ceea-467a-9c2b-1f2a3b4c5d6e";
        let id = stable_install_id(fallback);
        assert!(ServerId::parse(&id).is_ok());
        if !is_machine_derived() {
            assert_eq!(id, fallback);
        }
    }

    #[test]
    fn a_machine_derived_id_is_stable_across_calls() {
        // Only meaningful where a machine id exists. A container or a minimal
        // chroot has neither /etc/machine-id nor the dbus copy, and there every
        // call correctly returns the fallback it was handed instead.
        if !is_machine_derived() {
            return;
        }
        assert_eq!(stable_install_id("x"), stable_install_id("y"));
    }
}
