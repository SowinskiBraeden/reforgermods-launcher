//! Local Arma Reforger Workshop addon detection.
//!
//! # Status
//!
//! This module is the *abstraction plus a verified implementation* for reading
//! what the user already has installed. It is deliberately **not wired into the
//! milestone 1 UI**: the launcher shows no readiness indicator, because the one
//! piece the feature still needs — a supported way to trigger installation of
//! missing mods before joining — has not been verified. See
//! `docs/mod-readiness.md` for the full investigation.
//!
//! # What was verified
//!
//! Arma Reforger keeps downloaded Workshop content in the user's profile
//! directory, one directory per addon, named `<AddonName>_<16-hex-mod-id>`:
//!
//! ```text
//! %USERPROFILE%\Documents\My Games\ArmaReforger\addons\AKI_Core_62CCD69DD17E4F2F\
//!   ServerData.json      <- id, name, installed revision.version
//!   meta                 <- full Workshop metadata, dependencies, sizes
//!   addon.gproj          <- GUID + dependency GUIDs
//!   data.pak
//! ```
//!
//! `ServerData.json` is the cheapest reliable source: ~250 bytes, present in
//! every addon directory observed (235 of 235), and its `revision.version` is
//! byte-identical to the `version` reforgermods.net reports for the same mod.
//! Both files are written UTF-8 **with a byte-order mark**, which is why parsing
//! strips one before handing the text to serde.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Directory name of the addon store inside the Reforger profile directory.
const ADDONS_DIR_NAME: &str = "addons";
/// The per-addon file naming the installed revision.
const SERVER_DATA_FILE: &str = "ServerData.json";
/// Development override pointing at an addon store directly.
///
/// Useful when the game's profile is not where this platform would look — for
/// example running the launcher under WSL against a Windows install, or testing
/// against a copied store.
const ADDONS_DIR_ENV: &str = "RFM_ADDONS_DIR";

/// Errors from scanning the local addon store.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    /// No addon directory was found. Normal when Reforger has never been run,
    /// or when the profile lives somewhere non-default.
    #[error("no Arma Reforger addons directory found")]
    AddonsDirNotFound,
    #[error("could not read addons directory {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// One Workshop addon present on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledAddon {
    /// Workshop mod id, upper-case 16 hex characters, matching the API.
    pub id: String,
    /// Addon name as recorded locally. Untrusted text.
    pub name: String,
    /// Installed revision, e.g. `8.1.1`. Empty when the file did not name one.
    pub version: String,
}

/// How a server's required mod set compares to what is installed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModReadiness {
    /// Installed at exactly the version the server asks for.
    pub up_to_date: Vec<String>,
    /// Installed, but at a different version.
    pub outdated: Vec<String>,
    /// Not installed at all.
    pub missing: Vec<String>,
    /// Sum of `size` for missing mods whose size the API resolved. A floor, not
    /// a total: see `unresolved_bytes`.
    pub missing_bytes: u64,
    /// Missing mods whose download size the API could not resolve. While this is
    /// non-zero, `missing_bytes` understates the real download.
    pub unresolved_mods: u32,
}

impl ModReadiness {
    /// True when every required mod is installed at the required version.
    pub fn is_ready(&self) -> bool {
        self.outdated.is_empty() && self.missing.is_empty()
    }
}

/// A required mod, as reported by a server.
///
/// A narrow view of [`crate::api::models::ServerMod`] so this module does not
/// depend on the API model shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredMod {
    pub id: String,
    pub version: String,
    /// Download size in bytes when the API resolved it.
    pub size: Option<u64>,
}

/// A source of installed-addon information.
///
/// The trait exists so readiness logic can be tested against a fixed inventory,
/// and so a future platform with a different layout can be added without
/// touching callers.
pub trait AddonInventory {
    /// Every addon currently installed.
    fn installed(&self) -> Result<Vec<InstalledAddon>, ScanError>;
}

/// Reads the addon store from a directory on disk.
#[derive(Debug, Clone)]
pub struct FilesystemInventory {
    addons_dir: PathBuf,
}

impl FilesystemInventory {
    /// Scans `addons_dir` directly.
    pub fn new(addons_dir: impl Into<PathBuf>) -> Self {
        Self {
            addons_dir: addons_dir.into(),
        }
    }

    /// Locates the addon store for this machine.
    ///
    /// `RFM_ADDONS_DIR` overrides the search when set. Otherwise the profile
    /// locations for this platform are probed: the (often redirected)
    /// `Documents` directory on Windows, and Reforger's Proton prefix inside
    /// each Steam library on Linux. A platform with no known layout finds
    /// nothing and returns [`ScanError::AddonsDirNotFound`] — which the UI
    /// reports as "could not be checked", never as "nothing missing".
    pub fn discover() -> Result<Self, ScanError> {
        let explicit = std::env::var_os(ADDONS_DIR_ENV).map(PathBuf::from);
        addons_dir_candidates(explicit)
            .into_iter()
            .find(|p| p.is_dir())
            .map(Self::new)
            .ok_or(ScanError::AddonsDirNotFound)
    }

    /// The directory being scanned.
    pub fn addons_dir(&self) -> &Path {
        &self.addons_dir
    }
}

impl AddonInventory for FilesystemInventory {
    fn installed(&self) -> Result<Vec<InstalledAddon>, ScanError> {
        let entries = std::fs::read_dir(&self.addons_dir).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                ScanError::AddonsDirNotFound
            } else {
                ScanError::Read {
                    path: self.addons_dir.clone(),
                    source,
                }
            }
        })?;

        let mut addons = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                // The store also holds stray preview images and a `saves`
                // directory; anything without a ServerData.json is skipped below.
                continue;
            }
            let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(id_from_dir) = addon_id_from_dir_name(dir_name) else {
                continue;
            };
            // Prefer the id the file states; fall back to the directory name.
            let addon = std::fs::read_to_string(path.join(SERVER_DATA_FILE))
                .ok()
                .and_then(|raw| parse_server_data(&raw))
                .unwrap_or_else(|| InstalledAddon {
                    id: id_from_dir,
                    name: String::new(),
                    version: String::new(),
                });
            addons.push(addon);
        }
        addons.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(addons)
    }
}

/// Compares a server's required mods against an installed inventory.
pub fn readiness(required: &[RequiredMod], installed: &[InstalledAddon]) -> ModReadiness {
    let local: HashMap<String, &str> = installed
        .iter()
        .map(|a| (a.id.to_ascii_uppercase(), a.version.as_str()))
        .collect();

    let mut out = ModReadiness::default();
    for req in required {
        let key = req.id.to_ascii_uppercase();
        match local.get(&key) {
            Some(version) if *version == req.version => out.up_to_date.push(key),
            Some(_) => out.outdated.push(key),
            None => {
                match req.size {
                    Some(size) => out.missing_bytes += size,
                    None => out.unresolved_mods += 1,
                }
                out.missing.push(key);
            }
        }
    }
    out
}

/// Extracts the Workshop id from an addon directory name.
///
/// The id is the trailing `_`-separated group and must be exactly 16 hex
/// characters. Addon names themselves contain `_`, so the split is from the
/// right, and directories that do not match (`saves`, stray files) yield `None`.
pub fn addon_id_from_dir_name(name: &str) -> Option<String> {
    let (_, id) = name.rsplit_once('_')?;
    (id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit())).then(|| id.to_ascii_uppercase())
}

/// Parses an addon's `ServerData.json`.
///
/// The file is UTF-8 with a BOM; the BOM is stripped before parsing because
/// serde_json rejects it as unexpected input.
pub fn parse_server_data(raw: &str) -> Option<InstalledAddon> {
    #[derive(Deserialize)]
    struct Revision {
        #[serde(default)]
        version: String,
    }
    #[derive(Deserialize)]
    struct ServerData {
        id: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        revision: Option<Revision>,
    }

    let cleaned = raw.trim_start_matches('\u{feff}').trim();
    let data: ServerData = serde_json::from_str(cleaned).ok()?;
    let id = data.id.trim().to_ascii_uppercase();
    if id.len() != 16 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(InstalledAddon {
        id,
        name: data.name,
        version: data.revision.map(|r| r.version).unwrap_or_default(),
    })
}

/// Candidate addon-store locations, most likely first.
///
/// An `explicit` override short-circuits the search entirely: if the caller
/// named a directory, silently falling back to a different one would be worse
/// than reporting that the named one is not there.
fn addons_dir_candidates(explicit: Option<PathBuf>) -> Vec<PathBuf> {
    if let Some(dir) = explicit {
        return vec![dir];
    }
    let profile_suffix = Path::new("My Games").join("ArmaReforger");
    documents_roots()
        .into_iter()
        .map(|root| root.join(&profile_suffix).join(ADDONS_DIR_NAME))
        .collect()
}

/// Locations of the Windows `Documents` directory holding the Reforger profile.
///
/// `Documents` is frequently redirected — OneDrive being the common case — so
/// the registry's own answer is asked for first and the well-known layouts are
/// kept as fallbacks rather than assumed.
#[cfg(windows)]
fn documents_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    // Authoritative: this is where Explorer itself resolves Documents to, and it
    // follows a redirection to any path, not only the two guessed below.
    if let Some(personal) = documents_from_registry() {
        push_unique(&mut roots, personal);
    }
    if let Some(profile) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        push_unique(&mut roots, profile.join("Documents"));
        push_unique(&mut roots, profile.join("OneDrive").join("Documents"));
    }
    // OneDrive exports its own root; honour it rather than guessing the folder name.
    if let Some(onedrive) = std::env::var_os("OneDrive").map(PathBuf::from) {
        push_unique(&mut roots, onedrive.join("Documents"));
    }
    roots
}

/// Reads the user's real `Documents` location from the shell folder registry.
///
/// `Shell Folders` rather than `User Shell Folders`: the former stores the
/// already-expanded path, so no environment expansion is needed here.
#[cfg(windows)]
fn documents_from_registry() -> Option<PathBuf> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders")
        .ok()?;
    let personal: String = key.get_value("Personal").ok()?;
    let trimmed = personal.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

/// Locations of the Reforger profile's `Documents` directory on Linux.
///
/// There is no native Linux client: Reforger runs under Proton, so its
/// "`%USERPROFILE%\Documents`" is a directory inside the Wine prefix Steam keeps
/// per app, under whichever library folder the game was installed into:
///
/// ```text
/// <library>/steamapps/compatdata/1874880/pfx/drive_c/users/steamuser/Documents/
/// ```
///
/// Every known Steam root is probed, including the Flatpak one, and each root's
/// `libraryfolders.vdf` is read so a game on a second drive is found too — the
/// prefix lives in the library the game was installed into, which is routinely
/// not the one Steam itself lives in.
#[cfg(target_os = "linux")]
fn documents_roots() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    documents_roots_in(&home)
}

/// [`documents_roots`] against an explicit home directory.
///
/// Split out so the whole chain — Steam roots, `libraryfolders.vdf`, the prefix
/// layout — can be exercised against a synthetic tree, rather than only its
/// pieces in isolation.
#[cfg(target_os = "linux")]
fn documents_roots_in(home: &Path) -> Vec<PathBuf> {
    steam_libraries_in(home)
        .iter()
        .map(|library| proton_documents_dir(library))
        .collect()
}

/// Platforms whose layout has not been verified get no candidates at all, which
/// the UI reports as "could not be checked" — never as "nothing missing".
#[cfg(not(any(windows, target_os = "linux")))]
fn documents_roots() -> Vec<PathBuf> {
    Vec::new()
}

/// The Proton prefix's `Documents` directory for Reforger inside `library`.
#[cfg(target_os = "linux")]
fn proton_documents_dir(library: &Path) -> PathBuf {
    library
        .join("steamapps")
        .join("compatdata")
        .join(crate::launch::ARMA_REFORGER_APP_ID.to_string())
        .join("pfx")
        .join("drive_c")
        .join("users")
        // Proton always names the prefix user `steamuser`, regardless of the
        // real account name.
        .join("steamuser")
        .join("Documents")
}

/// Every Steam library folder on this machine, most likely first.
#[cfg(target_os = "linux")]
fn steam_libraries_in(home: &Path) -> Vec<PathBuf> {
    let mut libraries: Vec<PathBuf> = Vec::new();
    for root in steam_roots_in(home) {
        // The Steam root is itself a library.
        push_unique(&mut libraries, root.clone());
        let vdf = root.join("steamapps").join("libraryfolders.vdf");
        let Ok(raw) = std::fs::read_to_string(&vdf) else {
            continue;
        };
        for extra in library_paths_from_vdf(&raw) {
            push_unique(&mut libraries, extra);
        }
    }
    libraries
}

/// Well-known Steam installation roots under `home`.
///
/// `.steam/steam` and `.steam/root` are usually symlinks into one of the others;
/// they are probed anyway because they are the stable names, and duplicates are
/// dropped by path rather than by target.
#[cfg(target_os = "linux")]
fn steam_roots_in(home: &Path) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for relative in [
        ".steam/steam",
        ".steam/root",
        ".local/share/Steam",
        // Flatpak Steam keeps its own home directory.
        ".var/app/com.valvesoftware.Steam/.local/share/Steam",
    ] {
        push_unique(&mut roots, home.join(relative));
    }
    roots
}

/// Extracts library paths from `libraryfolders.vdf`.
///
/// Deliberately not a VDF parser. The file is Valve's KeyValues format and the
/// only thing needed from it is the `"path"` of each entry, so this reads those
/// lines and ignores the rest; a malformed or reorganised file yields fewer
/// candidates rather than an error.
#[cfg(target_os = "linux")]
fn library_paths_from_vdf(raw: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for line in raw.lines() {
        // `"path"\t\t"/mnt/games/SteamLibrary"`
        let Some(rest) = line.trim().strip_prefix("\"path\"") else {
            continue;
        };
        let mut fields = rest.split('"');
        let _separator = fields.next();
        let Some(value) = fields.next() else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        // KeyValues escapes backslashes; Windows-style paths appear in the file
        // on Windows installs and are harmless to unescape everywhere.
        paths.push(PathBuf::from(value.replace("\\\\", "\\")));
    }
    paths
}

/// Appends `path` unless it is already present, preserving search order.
#[cfg(any(windows, target_os = "linux"))]
fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real `ServerData.json`, BOM included, from
    /// `addons/AKI_Core_62CCD69DD17E4F2F`.
    const REAL_SERVER_DATA: &str = "\u{feff}{\n    \"id\": \"62CCD69DD17E4F2F\",\n    \"name\": \"AKI_Core\",\n    \"revision\": {\"version\":\"8.1.1\",\"gameVersion\":\"\",\"changelog\":\"\",\"corrupted\":false,\"dependencies\":[{\"assetId\":\"629B2BA37EFFD577\",\"assetName\":\"WCS_Armaments\",\"version\":\"8.1.2\"}],\"scenarios\":[],\"env\":7\n    }\n}";

    fn addon(id: &str, version: &str) -> InstalledAddon {
        InstalledAddon {
            id: id.into(),
            name: "n".into(),
            version: version.into(),
        }
    }

    fn required(id: &str, version: &str, size: Option<u64>) -> RequiredMod {
        RequiredMod {
            id: id.into(),
            version: version.into(),
            size,
        }
    }

    #[test]
    fn parses_a_real_server_data_file_including_its_bom() {
        let a = parse_server_data(REAL_SERVER_DATA).unwrap();
        assert_eq!(a.id, "62CCD69DD17E4F2F");
        assert_eq!(a.name, "AKI_Core");
        // This is the exact version api.reforgermods.net reports for this mod.
        assert_eq!(a.version, "8.1.1");
    }

    #[test]
    fn parses_without_a_bom_too() {
        let a = parse_server_data(REAL_SERVER_DATA.trim_start_matches('\u{feff}')).unwrap();
        assert_eq!(a.version, "8.1.1");
    }

    #[test]
    fn lowercase_ids_are_normalised_upwards() {
        let a =
            parse_server_data(r#"{"id":"62ccd69dd17e4f2f","revision":{"version":"1.0"}}"#).unwrap();
        assert_eq!(a.id, "62CCD69DD17E4F2F");
    }

    #[test]
    fn missing_revision_yields_an_empty_version_not_a_failure() {
        let a = parse_server_data(r#"{"id":"62CCD69DD17E4F2F","name":"x"}"#).unwrap();
        assert_eq!(a.version, "");
    }

    #[test]
    fn malformed_server_data_is_rejected() {
        for raw in [
            "",
            "not json",
            r#"{"name":"no id"}"#,
            // Wrong id length.
            r#"{"id":"ABC"}"#,
            r#"{"id":"62CCD69DD17E4F2FF"}"#,
            // Non-hex.
            r#"{"id":"ZZCCD69DD17E4F2F"}"#,
        ] {
            assert!(parse_server_data(raw).is_none(), "{raw:?}");
        }
    }

    #[test]
    fn addon_id_is_read_from_the_directory_name() {
        assert_eq!(
            addon_id_from_dir_name("AKI_Core_62CCD69DD17E4F2F").as_deref(),
            Some("62CCD69DD17E4F2F")
        );
        // Names contain underscores, so the split must be from the right.
        assert_eq!(
            addon_id_from_dir_name("B59-Shotguns-BETA_658B2B37C2C82AAE").as_deref(),
            Some("658B2B37C2C82AAE")
        );
        assert_eq!(
            addon_id_from_dir_name("ArmaverseFactionsReforged_6942069420694200").as_deref(),
            Some("6942069420694200")
        );
    }

    #[test]
    fn non_addon_directory_names_are_skipped() {
        // These are real entries that sit alongside addons in the store.
        for name in [
            "saves",
            "scenario0_576x324.jpg",
            "NoIdHere",
            "Short_ABC",
            "Bad_ZZZZZZZZZZZZZZZZ",
            "_",
            "",
        ] {
            assert!(addon_id_from_dir_name(name).is_none(), "{name:?}");
        }
    }

    #[test]
    fn readiness_reports_a_fully_installed_server_as_ready() {
        let installed = vec![
            addon("AAAAAAAAAAAAAAAA", "1.0"),
            addon("BBBBBBBBBBBBBBBB", "2.0"),
        ];
        let required = vec![
            required("AAAAAAAAAAAAAAAA", "1.0", Some(10)),
            required("BBBBBBBBBBBBBBBB", "2.0", Some(20)),
        ];
        let r = readiness(&required, &installed);
        assert!(r.is_ready());
        assert_eq!(r.up_to_date.len(), 2);
        assert_eq!(r.missing_bytes, 0);
    }

    #[test]
    fn readiness_separates_outdated_from_missing() {
        let installed = vec![
            addon("AAAAAAAAAAAAAAAA", "1.0"),
            addon("BBBBBBBBBBBBBBBB", "1.9"),
        ];
        let required = vec![
            required("AAAAAAAAAAAAAAAA", "1.0", Some(10)),
            required("BBBBBBBBBBBBBBBB", "2.0", Some(20)),
            required("CCCCCCCCCCCCCCCC", "3.0", Some(300)),
        ];
        let r = readiness(&required, &installed);
        assert!(!r.is_ready());
        assert_eq!(r.up_to_date, vec!["AAAAAAAAAAAAAAAA"]);
        assert_eq!(r.outdated, vec!["BBBBBBBBBBBBBBBB"]);
        assert_eq!(r.missing, vec!["CCCCCCCCCCCCCCCC"]);
        // Only missing mods need downloading; an outdated mod's delta is unknown.
        assert_eq!(r.missing_bytes, 300);
    }

    #[test]
    fn readiness_matching_is_case_insensitive_on_ids() {
        let installed = vec![addon("62ccd69dd17e4f2f", "8.1.1")];
        let required = vec![required("62CCD69DD17E4F2F", "8.1.1", None)];
        assert!(readiness(&required, &installed).is_ready());
    }

    #[test]
    fn unresolved_sizes_are_counted_separately_from_bytes() {
        let required = vec![
            required("AAAAAAAAAAAAAAAA", "1.0", None),
            required("BBBBBBBBBBBBBBBB", "1.0", Some(500)),
        ];
        let r = readiness(&required, &[]);
        assert_eq!(r.missing.len(), 2);
        assert_eq!(r.missing_bytes, 500);
        // The caller must be able to say "at least 500 bytes, plus 1 unknown".
        assert_eq!(r.unresolved_mods, 1);
    }

    #[test]
    fn a_vanilla_server_is_ready_against_an_empty_inventory() {
        let r = readiness(&[], &[]);
        assert!(r.is_ready());
        assert_eq!(r.missing_bytes, 0);
    }

    #[test]
    fn extra_installed_mods_do_not_affect_readiness() {
        let installed = vec![addon("AAAAAAAAAAAAAAAA", "1.0"), addon("ZZZZ", "9")];
        let required = vec![required("AAAAAAAAAAAAAAAA", "1.0", None)];
        assert!(readiness(&required, &installed).is_ready());
    }

    #[test]
    fn scanning_a_synthetic_store_finds_addons_and_ignores_noise() {
        let root = std::env::temp_dir().join(format!("rfm-modscan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let addons = root.join("addons");
        for (dir, body) in [
            (
                "AKI_Core_62CCD69DD17E4F2F",
                Some(REAL_SERVER_DATA.to_string()),
            ),
            (
                "Other_Mod_AAAAAAAAAAAAAAAA",
                Some(
                    r#"{"id":"AAAAAAAAAAAAAAAA","name":"Other","revision":{"version":"2.5"}}"#
                        .into(),
                ),
            ),
            // Valid directory name but no ServerData.json: still counted, since
            // the id is recoverable, but with no version.
            ("Broken_BBBBBBBBBBBBBBBB", None),
            // Not an addon directory at all.
            ("saves", None),
        ] {
            let path = addons.join(dir);
            std::fs::create_dir_all(&path).unwrap();
            if let Some(body) = body {
                std::fs::write(path.join(SERVER_DATA_FILE), body).unwrap();
            }
        }
        // A stray file alongside the directories.
        std::fs::write(addons.join("scenario0_576x324.jpg"), b"not a dir").unwrap();

        let found = FilesystemInventory::new(&addons).installed().unwrap();
        let ids: Vec<&str> = found.iter().map(|a| a.id.as_str()).collect();
        // Sorted by id: ASCII digits precede letters.
        assert_eq!(
            ids,
            vec!["62CCD69DD17E4F2F", "AAAAAAAAAAAAAAAA", "BBBBBBBBBBBBBBBB"]
        );
        let aki = found.iter().find(|a| a.id == "62CCD69DD17E4F2F").unwrap();
        assert_eq!(aki.version, "8.1.1");
        let broken = found.iter().find(|a| a.id == "BBBBBBBBBBBBBBBB").unwrap();
        assert_eq!(broken.version, "");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_explicit_directory_short_circuits_the_search() {
        let candidates = addons_dir_candidates(Some(PathBuf::from("/somewhere/addons")));
        assert_eq!(candidates, vec![PathBuf::from("/somewhere/addons")]);
    }

    #[test]
    fn without_an_override_only_profile_locations_are_probed() {
        for candidate in addons_dir_candidates(None) {
            // Every default candidate ends at the addon store, never at a
            // parent the scanner would misread.
            assert!(candidate.ends_with(ADDONS_DIR_NAME), "{candidate:?}");
            assert!(candidate.to_string_lossy().contains("ArmaReforger"));
        }
    }

    #[cfg(target_os = "linux")]
    mod linux_discovery {
        use super::super::*;

        /// A real `libraryfolders.vdf`, trimmed: two libraries, one of them on a
        /// second drive, with the surrounding keys the file actually carries.
        const VDF: &str = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"/home/madison/.local/share/Steam"
		"label"		""
		"contentid"		"1234567890123456789"
		"totalsize"		"0"
		"apps"
		{
			"1874880"		"27917287424"
		}
	}
	"1"
	{
		"path"		"/mnt/games/SteamLibrary"
		"label"		"games"
		"apps"
		{
			"221100"		"29249693425"
		}
	}
}
"#;

        #[test]
        fn every_library_path_is_read_from_the_vdf() {
            assert_eq!(
                library_paths_from_vdf(VDF),
                vec![
                    PathBuf::from("/home/madison/.local/share/Steam"),
                    PathBuf::from("/mnt/games/SteamLibrary"),
                ]
            );
        }

        #[test]
        fn non_path_keys_are_ignored_rather_than_misread() {
            // "label" and the app-size entries are the lines most likely to be
            // mistaken for a path by a loose parser.
            assert!(library_paths_from_vdf("\t\"label\"\t\t\"games\"").is_empty());
            assert!(library_paths_from_vdf("\t\t\"1874880\"\t\t\"27917287424\"").is_empty());
            assert!(library_paths_from_vdf("nonsense").is_empty());
            assert!(library_paths_from_vdf("").is_empty());
        }

        #[test]
        fn a_library_yields_the_proton_prefix_documents_directory() {
            let documents = proton_documents_dir(Path::new("/mnt/games/SteamLibrary"));
            let expected: PathBuf = [
                "/mnt/games/SteamLibrary",
                "steamapps",
                "compatdata",
                "1874880",
                "pfx",
                "drive_c",
                "users",
                "steamuser",
                "Documents",
            ]
            .iter()
            .collect();
            assert_eq!(documents, expected);
        }

        #[test]
        fn candidates_point_inside_a_proton_prefix() {
            // HOME is set in any normal environment, including the build
            // container; without it there is nothing to probe and that is fine.
            if std::env::var_os("HOME").is_none() {
                return;
            }
            let candidates = addons_dir_candidates(None);
            assert!(!candidates.is_empty());
            for candidate in &candidates {
                let shown = candidate.to_string_lossy();
                assert!(shown.contains("compatdata/1874880"), "{shown}");
                assert!(shown.contains("drive_c/users/steamuser"), "{shown}");
                assert!(shown.ends_with("My Games/ArmaReforger/addons"), "{shown}");
            }
        }

        #[test]
        fn the_flatpak_steam_root_is_probed_too() {
            let roots = steam_roots_in(Path::new("/home/madison"));
            assert!(
                roots
                    .iter()
                    .any(|r| r.to_string_lossy().contains("com.valvesoftware.Steam")),
                "{roots:?}"
            );
            // The stable symlink names and the real directory are distinct
            // paths, so all of them are probed.
            assert!(roots.len() >= 4, "{roots:?}");
        }

        /// The whole Linux chain against a synthetic install.
        ///
        /// The unit tests above each cover one link — the vdf, the prefix
        /// layout, the scanner. This one builds a Steam tree with the game on a
        /// *second* library, which is the case that actually breaks a naive
        /// implementation: the prefix lives in the library the game was
        /// installed into, not the one Steam itself lives in.
        #[test]
        fn a_game_on_a_second_library_drive_is_found_and_scanned() {
            let root = std::env::temp_dir().join(format!(
                "rfm-proton-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let home = root.join("home/madison");
            let steam = home.join(".local/share/Steam");
            let second_library = root.join("mnt/games/SteamLibrary");

            // Steam's own library, holding only the index that names the other.
            std::fs::create_dir_all(steam.join("steamapps")).unwrap();
            std::fs::write(
                steam.join("steamapps/libraryfolders.vdf"),
                format!(
                    "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n\
\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
                    steam.display(),
                    second_library.display()
                ),
            )
            .unwrap();

            // The addon store, inside the Proton prefix on the second library.
            let addons = proton_documents_dir(&second_library)
                .join("My Games")
                .join("ArmaReforger")
                .join(ADDONS_DIR_NAME);
            let addon = addons.join("AKI_Core_62CCD69DD17E4F2F");
            std::fs::create_dir_all(&addon).unwrap();
            // Written with the BOM the game really writes.
            std::fs::write(
                addon.join(SERVER_DATA_FILE),
                "\u{feff}{\"id\":\"62CCD69DD17E4F2F\",\"name\":\"AKI_Core\",\
\"revision\":{\"version\":\"8.1.1\"}}",
            )
            .unwrap();

            // Discovery, exactly as discover() performs it.
            let found = documents_roots_in(&home)
                .into_iter()
                .map(|documents| {
                    documents
                        .join("My Games")
                        .join("ArmaReforger")
                        .join(ADDONS_DIR_NAME)
                })
                .find(|candidate| candidate.is_dir());
            assert_eq!(
                found.as_deref(),
                Some(addons.as_path()),
                "store not located"
            );

            // And the scan of what discovery found.
            let installed = FilesystemInventory::new(found.unwrap())
                .installed()
                .unwrap();
            assert_eq!(
                installed,
                vec![InstalledAddon {
                    id: "62CCD69DD17E4F2F".into(),
                    name: "AKI_Core".into(),
                    version: "8.1.1".into(),
                }]
            );

            std::fs::remove_dir_all(&root).unwrap();
        }

        #[test]
        fn duplicate_library_paths_are_probed_once() {
            let mut paths = vec![PathBuf::from("/a")];
            push_unique(&mut paths, PathBuf::from("/a"));
            push_unique(&mut paths, PathBuf::from("/b"));
            assert_eq!(paths, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
        }
    }

    #[test]
    fn a_missing_store_reports_not_found_rather_than_an_io_error() {
        let missing = std::env::temp_dir().join("rfm-definitely-not-here-4a7f");
        let err = FilesystemInventory::new(&missing).installed().unwrap_err();
        assert!(matches!(err, ScanError::AddonsDirNotFound), "{err:?}");
    }
}
