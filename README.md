# reforgermods.net launcher

A fast, lightweight desktop companion for **Arma Reforger**, built on the
[reforgermods.net](https://reforgermods.net) API. It is a live server browser
that can put you straight into a server through Steam, without going through the
in-game lobby.

Unofficial. Not affiliated with or endorsed by Bohemia Interactive.

## Status

**0.1.0 — public beta.** The launcher browses and filters the live server list,
shows full detail for a selected server, joins directly through Steam, and keeps
favorites, recents and settings across restarts. Milestone 2 added population
history charts, a scenario filter, and mod readiness as reporting only; 0.1.0
adds packaging, signed auto-updates and a download page.

Beta artifacts are a per-user NSIS installer, a portable `.exe` and an Arch
package, published to `dl.reforgermods.net` and offered from
[reforgermods.net/launcher/](https://reforgermods.net/launcher/). They are **not
Authenticode-signed yet**, so Windows shows a SmartScreen warning on first run —
the pipeline is wired for Azure Trusted Signing and waiting on the identity.
See [`docs/distribution.md`](docs/distribution.md).

What works today:

- Live server browser over `GET /v2/servers`, with search, sort, region,
  platform, official / modded / BattlEye / password / offline filters, and
  page-local scenario and minimum-player filters
- **Sortable columns.** Clicking a heading sorts by it. `players` and `name` are
  ordered by the API across every matching server; queue, ping, mods, region and
  any reversed direction are orderings the API does not offer, so they reorder
  the loaded page — marked with a hollow arrow and named in the footer, never
  presented as a search of the whole fleet
- Selected-server inspector: scenario, population, queue, region, server FPS,
  and the full required-mod list with versions and sizes
- **Population history chart** over 6h / 24h / 7d / 30d, drawn as inline SVG with
  no charting library
- **Mod readiness, reporting only**: how many required mods are installed,
  outdated or missing, and at least how much is left to download
- **Measured ping** to the selected server by ICMP — a real round trip, not a
  region estimate
- Scenario imagery from the Bohemia CDN, host-allowlisted and CSP-enforced
- **Region latency in the browser**, sortable — eight ICMP probes cover a whole page
- Per-platform marks (PC / Xbox / PlayStation) matching the site, not a crossplay flag
- Fleet-wide player total in the title bar
- **Join Discord** where a server advertises an invite in its name
- **Discord Rich Presence**, showing the launcher as the running application
  (needs a Discord application; see [`docs/discord-presence.md`](docs/discord-presence.md))
- **Direct join**: `steam://run/1874880//<base64 payload>/`
- Favorites, recent servers and launch history, persisted locally
- Settings, and an anonymous, user-resettable install identifier
- **Background updates on Windows**, signature-verified, downloaded quietly and
  applied as the app closes — never mid-session. Linux installs are the distro's
  to update
- Short-lived response caching, and loading / empty / error / offline states
- Compact API status in the header, with dataset freshness

Not implemented, on purpose:

- **"Prepare mods & join".** Readiness is *reported*, never acted on. There is no
  verified way to install Reforger Workshop content from outside the game, so the
  launcher says what the game will do instead of offering a button it cannot
  honour. See [`docs/mod-readiness.md`](docs/mod-readiness.md).
- Accounts, uptime figures, mod browsing, background tray, auto-start.

## Stack

| Layer | Choice | Why |
| --- | --- | --- |
| Shell | Tauri 2 | Native WebView2 on Windows; no bundled browser runtime |
| Native | Rust | Steam launching, HTTP, persistence, local filesystem inspection |
| UI | Preact + `@preact/signals` + TypeScript | ~4 KB runtime, JSX in plain `.tsx`, and per-component signal subscriptions so the one-second freshness clock re-renders the status pill and not the 500-row table |
| Build | Vite 7 | |

The production bundle is **51 KB of JavaScript (17 KB gzipped)** and **14 KB of
CSS**, in one chunk. The history chart is hand-rolled inline SVG — a charting
library would have cost more than the rest of the UI combined.

### Layout

```text
crates/rfm-core/        Pure Rust. No Tauri dependency, so it is unit-testable directly.
  api/                  HTTP client, typed models, query builder, error taxonomy
  launch.rs             Server id validation -> join payload -> Steam URI
  store.rs              Settings, favorites, recents, launch history
  cache.rs              Bounded TTL cache of response bodies
  modscan.rs            Local Workshop addon detection
  readiness.rs          Required mods vs installed; reporting only, never acts
  ping.rs               ICMP latency; address validation is the security boundary
  presence.rs           Discord Rich Presence text, within Discord's limits

src-tauri/              The application shell. Window, commands, wiring. Thin.
  src/commands.rs       The entire IPC surface
  src/state.rs          Shared state
  src/update.rs         Background updates, Windows only; applied as the app exits
  capabilities/main.json
  tauri.signing.conf.json  Adds only signCommand; merged over the config in CI

src/                    UI
  lib/                  types, IPC wrappers, signals, pure formatters, chart geometry
  components/           title bar, status pill, sidebar, filter bar, server table,
                        inspector, history chart, readiness note, platform badges
  views/                browse, favorites, recent, settings

docs/api-contract.md          Endpoints used, and what is deliberately not used
docs/mod-readiness.md         The mod-readiness investigation and its findings
docs/client-identification.md Install-identity spec and API-side implementation notes
docs/discord-presence.md      Discord Rich Presence setup and behaviour
docs/distribution.md          Installer, code signing, release procedure, R2 layout
packaging/arch/               PKGBUILD and desktop entry for the Arch package
packaging/windows/sign.cmd    Authenticode signing, called by Tauri during bundling

scripts/version.sh            The version, in one place: show / check / set
scripts/release-manifest.py   releases.json (the site) and latest.json (the updater)
scripts/build-windows.sh      Cross-compile the bare .exe from Linux
scripts/build-arch.sh         Native Arch package, built in a container
.github/workflows/release.yml Tag -> build, sign, publish to R2
```

## Prerequisites

- **Rust** stable (1.82+) and **Node.js** 20+
- **Windows** (the primary target): the WebView2 runtime, which ships with
  Windows 11 and current Windows 10, plus the MSVC build tools
- **Linux** (development, and Arch packaging): the Tauri system dependencies —
  `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`,
  `librsvg2-dev`, `libsoup-3.0-dev`, `build-essential`

The TLS backend is chosen per target: schannel on Windows (no bundled C crypto
library, and the machine's own trust store), rustls elsewhere.

Installed-mod detection finds Reforger's addon store on both platforms: the
(often redirected) `Documents` directory on Windows, read from the shell folder
registry, and the Proton prefix inside each Steam library on Linux — there is no
native Linux client, so the game's `Documents` is
`<library>/steamapps/compatdata/1874880/pfx/drive_c/users/steamuser/Documents`.
`RFM_ADDONS_DIR` overrides the search for an install neither layout describes.

## Development

```sh
npm install
npm run tauri dev          # run the app
```

Frontend alone, in a browser (IPC calls will fail, but layout iterates fast):

```sh
npm run dev
```

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p rfm-core                 # 168 unit tests, no network
npm run check                          # tsc --noEmit
npm test                               # 82 tests, including a jsdom shell smoke test
scripts/version.sh check               # the four version copies agree
```

Contract checks against the live API, ignored by default:

```sh
# Serially: the public rate limit is 60 requests/minute, and these tests
# fetch server detail in loops.
cargo test -p rfm-core --test live_api -- --ignored --test-threads=1
```

To exercise the mod scanner against a real Arma Reforger addon store:

```sh
RFM_ADDONS_DIR="$HOME/Documents/My Games/ArmaReforger/addons" \
  cargo test -p rfm-core --test live_api -- --ignored readiness_against_a_real
```

The ping tests send ICMP echo requests to live game servers; they are part of the
same ignored-by-default suite.

## Build

```sh
npm run tauri build
```

Produces a per-user NSIS installer under `target/release/bundle/nsis/`, plus a
detached `.sig` when an updater signing key is present in the environment. MSI is
deliberately not built: it needs admin rights and doubles the release for an
audience a game utility does not have.

Releases are cut from a `v*` tag by `.github/workflows/release.yml`, which builds
on Windows, signs, and publishes to `dl.reforgermods.net`. The version lives in
one place and is propagated by `scripts/version.sh`:

```sh
scripts/version.sh show                # the canonical version
scripts/version.sh set 0.1.1           # rewrite all four copies
```

The full procedure, the signing state and the R2 layout are in
[`docs/distribution.md`](docs/distribution.md).

### Cross-building a Windows binary from Linux

No Visual Studio, and no root:

```sh
scripts/build-windows.sh            # debug, shows a console with diagnostics
scripts/build-windows.sh --release  # optimised, no console
```

`cargo-xwin` fetches the MSVC CRT and Windows SDK from Microsoft into
`~/.cache/cargo-xwin`, and LLVM's `clang-cl`/`lld-link` replace `cl.exe` and
`link.exe`; the script unpacks those from Debian packages into a local prefix.
The result is a standalone `.exe` — the frontend is embedded at compile time —
that needs only the WebView2 runtime, which ships with current Windows.

### Building an Arch Linux package

```sh
scripts/build-arch.sh               # target/arch/*.pkg.tar.zst
scripts/build-arch.sh --no-check    # skip the rfm-core test suite
```

The build runs inside an `archlinux:base-devel` container, so the binary links
Arch's own glibc and WebKit rather than the build machine's. This is not
fussiness: a Tauri binary built on Debian links `libwebkit2gtk-4.1` against
Debian's ABI, and handing that to an Arch user produces a blank window or a
missing-symbol crash. Docker is only being used as a clean Arch userspace —
nothing is deployed.

The tester installs it with `sudo pacman -U <file>`; pacman pulls
`webkit2gtk-4.1` and the rest from the declared dependencies.

Two things the PKGBUILD has to work around, both of which fail late and
confusingly if forgotten:

- `options=('!lto')` — makepkg puts `-flto=auto` in `CFLAGS`/`LDFLAGS` by
  default, which makes `aws-lc-sys` (the C crypto behind rustls) compile to GCC
  LTO bitcode that the linker cannot resolve. The build dies at the final link
  with a page of undefined `aws_lc_*` symbols. Rust-level LTO is unaffected; it
  comes from `[profile.release]`.
- `cmake` in `makedepends` — `aws-lc-sys` needs it, and `base-devel` does not
  include it.

`build()` asserts the result is a production build before packaging, for the
same reason `build-windows.sh` does: a dev build installs perfectly and then
shows `ERR_CONNECTION_REFUSED` on the tester's machine.

### Nvidia under Wayland

WebKitGTK's DMABUF renderer and the proprietary Nvidia driver disagree about
buffer import, and GTK aborts before the window is ever mapped:

```text
Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display.
```

`main.rs` detects that combination at startup and sets
`WEBKIT_DISABLE_DMABUF_RENDERER=1` for itself. The detection is deliberately
narrow — Nvidia loaded, a Wayland session, GDK not already pinned to X11, and no
existing value to override — because the fallback costs GPU acceleration for
anyone who does not need it. `should_disable_dmabuf` is a pure function with
tests covering each condition.

To override, set the variable yourself; any value is honoured, including `0`.

## Direct joining

Given a server UUID, the launcher builds:

```text
{"id":"<uuid>","inviteToken":""}
  -> UTF-8 bytes
  -> standard base64
  -> trailing "=" removed
  -> steam://run/1874880//<payload>/
```

### The doubled slash

`steam://run/<appid>//<args>/` is Steam's form for passing launch arguments, and
the **empty path segment** is what marks the payload as one. With a single
slash Steam can start the game without forwarding the payload, so the player
lands on the main menu and the join request is silently lost. A test pins this
shape, because the difference is one character and the failure looks like a
successful launch.

**Not yet confirmed against a live client.** The single-slash form was observed
to launch without joining; the doubled slash is the current best understanding
of the fix.

A join only carries the payload when the game actually starts. If Arma Reforger
is already running, Steam may focus the existing process and drop the arguments,
so this must be tested from a fully closed game.

`steam://run/1874880` (no payload) is the plain "Launch Arma Reforger" action and
is unaffected.

### An alternative route

The game also registers its own URL protocol. Its Steam install script
(`steamapps/common/Arma Reforger/installscript.vdf`) writes:

```text
HKCU\SOFTWARE\Classes\ArmaReforger\shell\open\command
  "<dir>\ArmaReforger_BE.exe" -exe ArmaReforgerSteam.exe
  "-addonsDir=<dir>\addons" -uri="%1"
```

`reforger_join_uri()` builds `ArmaReforger://<payload>`, which reaches the
launcher as `-uri=` without depending on how Steam forwards arguments. It is
implemented, tested and unused — a second candidate if the Steam form does not
join.

This lives in [`crates/rfm-core/src/launch.rs`](crates/rfm-core/src/launch.rs)
and is pure and directly tested, including against the verified reference
vector. No shell is involved at any point: the URI is handed to the platform's
URI opener, and the only value interpolated into it is a `ServerId`, which cannot
be constructed without passing UUID validation.

## Ping

A browser cannot measure round-trip latency to a game server; a native launcher
can, and this is one of the reasons for building one.

**TCP connect timing was tried and rejected.** Connecting to a server's UDP game
port "succeeded" in 12–18 ms — and so did port 9 and port 47123, which are
closed. Some middlebox completes every handshake, so TCP timing yields
confident-looking numbers that are entirely fictional.

ICMP echo works. Measured against 12 live servers, 11 answered, with latencies
coherent by region (Los Angeles 41 ms, New York 58–83 ms, Frankfurt 154–184 ms,
Sydney 173–179 ms). The launcher sends three probes and displays the **median**,
so one delayed packet does not move the figure.

- Windows uses `IcmpSendEcho` via the IP Helper API — **no elevation required**.
- Unix uses an unprivileged ICMP datagram socket.
- The **selected** server is measured exactly, in the inspector.
- The **browser column** shows latency to each server's *region*. `/v2/servers`
  publishes no per-server address — only `/v2/servers/{id}` does — so pinging a
  100-row page would cost 100 detail requests against a 60/minute limit. Every
  row does carry `pingSiteId`, and there are only eight sites, so eight probes
  cover the list. Region and server latency agree within roughly 20 ms, which is
  enough to sort by and is what the game's own browser shows.
- Roughly a third of servers filter ICMP. Those show "no reply" rather than a
  region-level estimate dressed up as a measurement.

## Security posture

- Everything from the API — server names, scenario names, mod names, URLs — is
  treated as untrusted. It is rendered as text content and never interpolated
  into a shell command, a URI, or markup.
- Server ids are validated before they reach a URL, on both the launch path and
  the "open on reforgermods.net" path. The latter builds its URL from the
  validated id rather than from the API's own `modURL` field.
- No API key or credential is embedded. The launcher is a public client and uses
  anonymous access only. The one value it sends that persists across requests is
  a random install identifier, visible and resettable in Settings; see
  [`docs/client-identification.md`](docs/client-identification.md).
- The API origin is not user-configurable, so a tampered settings file cannot
  redirect requests — or the install identifier — to another host.
- Server addresses from the API are validated to public literal IPs before any
  packet is sent, so remote data cannot drive DNS or probe the user's LAN.
- Image URLs are checked against a single-host allowlist before rendering, with
  the page CSP enforcing the same list independently.
- The UI is granted `core:default` and nothing else — no filesystem, shell, HTTP
  or opener permission. URIs are opened from Rust, after validation.
- A strict CSP allows `'self'` plus scenario thumbnails from the Bohemia CDN.

## Relationship to reforgermods.net

This is a client. All server and mod data comes from the public
`https://api.reforgermods.net` v2 API; the launcher holds no data of its own
beyond local preferences and caches, and duplicates no backend logic. See
[`docs/api-contract.md`](docs/api-contract.md).
