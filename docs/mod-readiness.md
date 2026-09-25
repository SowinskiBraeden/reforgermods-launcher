# Mod readiness: investigation findings

**Status: investigated, implemented as reporting only. No action is attached, and none should be until a supported install mechanism exists.**

The launcher mockup shows an installed / outdated / missing mod breakdown, a
download size, and a "Prepare mods & join" action. This document records what of
that is actually achievable today. Measurements were taken on 2026-09-24 against
a real Arma Reforger installation (game build `24903726`, client `1.8.0.13`) and
the live reforgermods.net API.

## Summary

| Question | Answer |
| --- | --- |
| Can installed Workshop mods be detected locally? | **Yes, reliably.** |
| Can installed *versions* be detected? | **Yes**, and they match the API exactly. |
| Does the API expose the versions a server requires? | **Yes**, with sizes. |
| Can missing-mod installation be triggered from outside the game? | **Not verified. No supported mechanism found.** |
| Does the game handle missing mods during the Steam join flow? | **Yes** — this is what makes the launcher useful without the above. |

The first three make a readiness *indicator* feasible, and it now ships. The
fourth is what "Prepare mods & join" would need, and it is the one that does not
hold up — so the launcher reports and does not offer to act.

## Where Reforger stores Workshop content

Downloaded addons live in the user's profile directory, **not** in the Steam
application directory and **not** in Steam Workshop storage (Reforger uses
Bohemia's own Workshop CDN, so `steamapps/workshop` is empty for app 1874880):

```text
%USERPROFILE%\Documents\My Games\ArmaReforger\
  addons\<AddonName>_<16-HEX-MOD-ID>\
    ServerData.json       id, name, installed revision version
    meta                  full Workshop metadata: versions, dependencies, sizes
    addon.gproj           GUID and dependency GUIDs
    data.pak              content
    resourceDatabase.rdb
  logs\
  profile\
```

`Documents` is commonly redirected to OneDrive, so the implementation asks the
shell folder registry (`HKCU\...\Explorer\Shell Folders`, `Personal`) first —
which follows a redirection to any path — and keeps
`%USERPROFILE%\Documents`, `%USERPROFILE%\OneDrive\Documents` and
`%OneDrive%\Documents` as fallbacks.

### On Linux

There is no native Linux client: Reforger runs under Proton, so the same profile
directory lives inside the Wine prefix Steam keeps per app, in whichever library
folder the game was installed into:

```text
<library>/steamapps/compatdata/1874880/pfx/drive_c/users/steamuser/Documents/
  My Games/ArmaReforger/addons/
```

Everything below this point — the naming rule, the BOM, `ServerData.json`, the
version comparison — is identical, because it is the same Windows game writing
the same files; only the path to them differs.

The scanner probes every known Steam root (`~/.steam/steam`, `~/.steam/root`,
`~/.local/share/Steam`, and the Flatpak
`~/.var/app/com.valvesoftware.Steam/.local/share/Steam`) and reads each one's
`steamapps/libraryfolders.vdf`, because the prefix lives in the library the game
was installed into — routinely not the one Steam itself lives in. `steamuser` is
Proton's fixed prefix account name, not the user's.

## Reliability of local detection

Measured against a real store of **238 directory entries**:

- 235 were addon directories; all 235 contained both `ServerData.json` and `meta`.
- 3 were not addons (a `saves` directory and two stray preview JPEGs) and are
  excluded by the `<Name>_<16 hex>` naming rule.
- Coverage of `ServerData.json`: **235 / 235**.

`ServerData.json` is the cheapest source — roughly 250 bytes — and contains
exactly what is needed:

```json
{"id":"62CCD69DD17E4F2F","name":"AKI_Core","revision":{"version":"8.1.1", ...}}
```

Two details that matter in practice:

- Both `ServerData.json` and `meta` are written **UTF-8 with a byte-order mark**.
  `serde_json` rejects a leading BOM, so it must be stripped before parsing.
- The mod id is also recoverable from the directory name. Addon names contain
  underscores, so the id is the *last* `_`-separated group, and it must be
  validated as 16 hex characters rather than simply taken.

## Version agreement with the API

`GET /v2/servers/{id}` returns each required mod with an exact `version` and,
where resolved, a `size` in bytes. Comparing that to the local store:

| Server | Required | Up to date | Outdated | Missing | Missing download |
| --- | ---: | ---: | ---: | ---: | ---: |
| `[OGR2] Old Guard Revival 2` | 122 | **122** | 0 | 0 | 0 |
| `[NA2] TWS UHC Ukraine v Russia` | 173 | 26 | 1 | 146 | 14.42 GiB |
| `LA FEDERACIÓN: ESTADO FALLIDO` | 91 | 15 | 0 | 76 | 15.59 GiB |
| `[BnB1xCOC] Beers an Bipods` | 136 | 39 | 1 | 96 | 8.17 GiB |
| `[EU1] SlavicWar` | 197 | 36 | 1 | 160 | 12.85 GiB |
| `UNSC Battlegroup Athens` | 104 | 12 | 1 | 91 | 19.09 GiB |

The first row is a server the machine's owner plays: **122 of 122 mods matched
on both id and exact version string**, with zero false negatives. Version strings
are byte-identical between `ServerData.json` and the API — no normalisation was
needed. The remaining rows confirm the missing and outdated paths produce
sensible figures rather than degenerate ones.

One caveat on sizes: `modSummary.unresolvedCount` is often non-zero on
`/v2/servers/{id}/mods` (that endpoint skips size enrichment unless asked), while
the detail endpoint resolves them. Any download figure must be reported as a
floor — "at least *N*, plus *k* unknown" — not a total.

## Triggering installation: not solved

This is where the feature stops.

- Reforger mods are **not** Steam Workshop items. `steam://` has no verb that can
  install them, and `steamapps/workshop` holds nothing for app 1874880.
- No documented command-line switch or URI on the Reforger client was found that
  installs a mod set without entering the game.
- The `meta` file is written *by* the game; treating it as an input — fabricating
  entries to make the client fetch content — is unsupported, undocumented, and
  would risk corrupting a user's addon store.

What does work is the flow the launcher already uses: `steam://run/1874880//<payload>/`
starts the game with a join request, and **the game itself resolves the missing
dependencies**, prompting for and downloading them before connecting. A user with
none of a server's 173 mods can still press "Join server"; the game handles it.

That is why milestone 1 ships the direct join and no readiness panel.

## What is implemented

`crates/rfm-core/src/modscan.rs` — detection:

- `AddonInventory` — the trait, so readiness logic can be tested against a fixed
  inventory and a different platform layout can be added without touching callers.
- `FilesystemInventory` — scans the addon store; `discover()` locates it.
- `parse_server_data` / `addon_id_from_dir_name` — the two parsing rules above.
- `readiness(required, installed)` — up to date / outdated / missing, plus
  `missing_bytes` and `unresolved_mods`.

`crates/rfm-core/src/readiness.rs` — reporting. Bridges the API mod list to the
local inventory and produces a `ReadinessReport` for the UI. Two rules are
enforced here rather than in the UI, so they cannot be lost in a redesign:

- A scan that did not run reports `available: false` with every count zeroed.
  `is_ready()` is false in that state: **not knowing is not being ready.**
- A mod whose size the API could not resolve is counted in `unresolved_mods`
  rather than contributing zero bytes, so `missing_bytes` is always a floor that
  the UI can honestly label "at least".

`src/lib/format.ts` — `readinessSummary()` builds the sentence, and is unit
tested against the exact expected string. `src/components/ReadinessNote.tsx`
renders it as a line of text and a count strip. There is no button.

## The copy

```text
96 of 136 mods missing · at least 8.17 GiB · Arma Reforger will download these when you join
1 of 136 mods out of date · Arma Reforger will update these when you join
All 136 mods installed and up to date.
No local Arma Reforger addon folder found, so installed mods could not be checked.
```

"at least" appears whenever any required mod's size is unresolved. The closing
clause names the actor — the game, not the launcher — because that is what
actually performs the download.

## Verified end to end

`cargo test -p rfm-core --test live_api -- --ignored readiness_against_a_real`
runs the real scanner against a real addon store (`RFM_ADDONS_DIR`) and the live
API. Against the 235-addon store described above:

```text
local store: 235 addons
[AU] FTA #1 | From The Ashes                   req=123 ok= 59 old= 6 miss= 58 >= 1.65 GiB
[AU] FTA #2 | From The Ashes                   req=127 ok= 61 old= 6 miss= 60 >= 2.84 GiB
[NA6] W.C.S. Realism NATO/RUS Belleau Wood     req= 83 ok= 64 old= 4 miss= 15 >= 1.30 GiB
[OG9] Old Guys 9 | Fallujah Iraq               req=114 ok= 85 old=29 miss=  0 >= 0.00 GiB
[EU] LuckyGames Hardcore | Bakhmut             req=106 ok= 35 old= 9 miss= 62 >= 5.10 GiB
[EU2] W.C.S. Realism NATO/RUS Serhiivka        req= 80 ok= 62 old= 4 miss= 14 >= 0.86 GiB
```

The test asserts the three buckets partition the required list exactly. The OG9
row — 0 missing but 29 outdated — is the case that exercises the "update these"
wording rather than "download these".

## Still not solved

Triggering installation. Nothing above changes that, and the "Prepare mods &
join" action stays out until a supported mechanism exists.
