# Distributing the launcher from reforgermods.net

**Implemented, with one thing outstanding.** The decisions below are made, the
pipeline exists (`.github/workflows/release.yml`) and 0.1.0 is built. The gap is
Authenticode: the release job is wired for Azure Trusted Signing but the
credentials do not exist yet, so the first beta artifacts are **unsigned** and
will show the SmartScreen warning. Everything else — NSIS per-user installer,
signed auto-updates, R2 hosting, the download page — is in place.

See [Release procedure](#release-procedure) for how to cut one, and
[What is still outstanding](#what-is-still-outstanding) for the gap.

## The shape, in one line

An **NSIS per-user installer** as the primary download, built on **Windows in
CI**, signed with **Azure Trusted Signing**, hosted on **Cloudflare R2** at
`dl.reforgermods.net`, with the **Tauri updater** wired so nobody ends up running
a stale API client.

## 1. Installer format

Tauri can produce three Windows artifacts. They are not equivalent:

| Format | Admin needed | Notes |
| --- | --- | --- |
| **NSIS** (`.exe`) | No, with `installMode: currentUser` | Start Menu entry, uninstaller, updater support, smallest |
| **MSI** (WiX) | Yes | Enterprise deployment, Group Policy. Irrelevant for a game tool |
| Portable `.exe` | No | No Start Menu, no uninstaller, SmartScreen prompt every time |

**NSIS, per-user.** It is already configured. Per-user matters: a UAC prompt on
a game utility loses people, and there is no reason to write outside the user's
profile — the launcher already stores everything under the per-user config
directory.

**Done:** `targets` is `["nsis"]`. MSI was removed — it doubled release size and
build time for an audience that will not use it.

The **portable `.exe` is published as a secondary download** for people who will
not run installers. It costs nothing: it is the same binary the installer wraps,
and the release job copies it straight out of the build.

### WebView2

The installer's `webviewInstallMode` decides what happens when the runtime is
missing. `downloadBootstrapper` (the default) adds ~2 MB and fetches the runtime
only when absent. Windows 11 and current Windows 10 already ship it, so most
users never notice. `embedBootstrapper` or `offlineInstaller` only make sense if
offline installs matter.

## 2. Code signing — the decision that actually matters

Unsigned, every download shows **"Windows protected your PC"** and requires
More info → Run anyway. For a launcher whose point is promotion, that is the
single biggest conversion loss in the whole pipeline.

**What changed:** EV certificates used to grant immediate SmartScreen
reputation. **Microsoft removed that in 2024.** An EV certificate now builds
reputation the same way an OV one does, so paying EV prices purely to skip
SmartScreen is no longer justified.

| Option | Cost | Verdict |
| --- | --- | --- |
| Nothing | £0 | SmartScreen warning on every download. Not viable for a promoted tool |
| OV certificate | ~£200–400/yr | Signs fine; reputation still has to build |
| EV certificate | ~£300–600/yr + token/HSM | No longer buys instant reputation. Hard to justify |
| **Azure Trusted Signing** | **$9.99/mo** (5,000 signatures) | Cheapest credible route; no hardware token; fits CI cleanly |

**Recommended: Azure Trusted Signing** (renamed Azure Artifact Signing).
Eligibility is verified US / Canada / EU / UK businesses and self-employed
individuals — `cedarline.digital` should qualify, and the organisation identity
is what appears in the UAC and SmartScreen dialogs.

Be realistic about reputation: signing removes the *unknown publisher* framing,
but SmartScreen reputation still accrues with download volume. Expect some
friction on the first releases regardless of route. The mitigation is to keep
one stable signing identity rather than rotating certificates, so reputation
accumulates against one publisher.

**State:** chosen, wired, not yet active. The release job signs through
`bundle.windows.signCommand`, supplied by `src-tauri/tauri.signing.conf.json` and
run by `packaging/windows/sign.cmd`, which shells out to `trusted-signing-cli`.

Signing has to happen *during* bundling rather than after it, which is why it is
a `signCommand` and not a later CI step: the updater artifact is the installer
plus a detached signature over it. Sign the installer afterwards and the
signature describes a file nobody will download, and every client rejects the
update.

The job checks for `AZURE_CLIENT_ID` and `RFM_SIGN_ENDPOINT` and, when they are
absent, builds unsigned and prints a warning to the run summary rather than
failing. That is deliberate: it keeps the beta shippable while the Azure identity
is sorted out, and it makes the unsigned state loud instead of silent. See
[What is still outstanding](#what-is-still-outstanding).

## 3. Build and release pipeline

Cross-compiling from Linux works for the bare `.exe` (`scripts/build-windows.sh`)
and is fine for development, but the release build should run on Windows:

- NSIS bundling and signing both want Windows tooling.
- The signed artifact should be produced by CI, not a laptop, so releases are
  reproducible and the signing credential lives in one place.

**Done:** `.github/workflows/release.yml`. A `windows-latest` job on a `v*` tag
runs the full check suite, builds, signs when it can, and uploads the artifacts;
a second job generates the manifests and publishes to R2. It is stored under
`.github/workflows` so both GitHub Actions and Gitea Actions pick it up, matching
the other two repositories.

The job refuses to build unless the tag and every version in the tree already
agree (`scripts/version.sh check "$tag"`), so a mistagged release fails in the
first thirty seconds rather than after producing artifacts.

**Hosting: Cloudflare R2**, served at `dl.reforgermods.net`. It sits on the
account that already runs the site, keeps the project free of a GitHub
dependency, and downloads never touch the API's budget. The trade is that the
bandwidth is ours; for an installer of about 2 MB that is not a real cost at beta
volumes. See [R2 setup](#r2-setup) for the one-time configuration.

Versioned paths are immutable and only two small files move — `/releases.json`
and `/updates/latest.json` — which is what makes a rollback a matter of
re-publishing one file rather than moving binaries around.

## 4. Auto-update

`tauri-plugin-updater` checks a JSON manifest, verifies a signature against a
public key baked into the build, and applies the update.

This matters more than usual here: the launcher is an API client. When
`/v2/servers` changes shape, users on an old build get a broken browser and
blame the site. An updater turns that from a support problem into a non-event.

Needs a keypair (`tauri signer generate`) — the private key is a CI secret, the
public key ships in `tauri.conf.json`. Distinct from code signing; both are
needed and they solve different problems.

**Done**, in `src-tauri/src/update.rs`. The keypair exists and its public half is
in `tauri.conf.json`; the private key is **not** in this repository and has to be
added to CI as `TAURI_SIGNING_PRIVATE_KEY` (see
[Release procedure](#release-procedure)).

The behaviour is exactly as proposed, and it is three steps rather than
`download_and_install` for a reason:

1. check and download in the background, so nothing blocks startup;
2. hold the downloaded installer until the session ends;
3. run it as the app exits, with `restart_after_install(false)`.

Nobody is interrupted mid-session and nobody is asked to make a decision about a
patch release; the update is simply there next time. `Update::install` calls
`std::process::exit(0)` on Windows after launching the installer, which is
harmless at exit and would be a hard kill at any other moment — which is the
other reason the install is pinned to that exact point.

Every failure path is silent: no endpoint, no network, a malformed manifest or a
bad signature all leave the app as it was. `RFM_DEBUG=1` is how it becomes
visible.

**Windows only.** The dependency is scoped to Windows in `Cargo.toml`, so the
module does not exist in a Linux build. The Arch package belongs to pacman, and
an app that rewrites its own files under `/usr/bin` fights the package manager.

## 5. Download page

**Done**, in the `reforgermods-web` repository: `/launcher/` with
`public/static/launcher.js` and `public/static/launcher/releases.json`.

- Detects the visitor's platform and offers the matching build as the primary
  action; an unrecognised platform gets the full table rather than a guess.
- Lists every download with its size and SHA-256.
- States the WebView2 requirement as a footnote — nearly nobody needs it.
- The page ships a working pre-release state in its HTML and the script only ever
  upgrades it, so a failed fetch leaves an honest panel rather than a dead button.

The page reads **its own copy** of `releases.json`, served from the site origin
rather than from R2. That avoids a cross-origin dependency on the download host
for a page that must render, but it means a release is not visible until that
file is updated and the site is deployed. The release job says so in its summary.

## 6. Versioning

**Done:** `scripts/version.sh`.

    scripts/version.sh show          print the canonical version
    scripts/version.sh check [tag]   fail if any copy disagrees
    scripts/version.sh set 0.2.0     rewrite every copy

The canonical value is `[workspace.package] version` in the root `Cargo.toml`,
because that is the one the running binary reports: `rfm-core` builds its
User-Agent from `CARGO_PKG_VERSION`, so the API's per-version rollups are keyed
off it whatever the other files say.

It still has to be repeated in four places — the workspace manifest,
`package.json`, `tauri.conf.json` and the PKGBUILD — because no two of those
tools read each other's format. `check` is what makes the repetition safe, and
the release job runs it against the tag before building anything. An updater
manifest that disagrees with the installed version either loops forever or never
fires, and neither failure is obvious from the outside.

## 7. Linux

Not a distribution channel yet — but the app does build and run there, and
`scripts/build-arch.sh` produces a native Arch package
(`target/arch/*.pkg.tar.zst`) for handing to a tester. It builds inside an
`archlinux:base-devel` container so the binary links Arch's WebKit and glibc
rather than the build host's; `packaging/arch/PKGBUILD` holds the recipe and
README covers the two makepkg workarounds it needs.

Verified end to end: installed into a clean `archlinux:base` container with
`pacman -U`, every dependency resolved from the declared `depends`, and the app
started under Xvfb, loaded the live server list and resolved pings over
unprivileged ICMP.

**Changed for the beta:** the Arch package is now published alongside the Windows
builds rather than handed over per tester. It appears on the download page for
visitors the page detects as Arch, and in the table for everyone else.

That is one distribution, not a Linux strategy. `.deb`, AppImage and an AUR
package are all still unbuilt, and `public/static/launcher.js` already knows
those platform ids so adding them is a packaging job, not a web one. The updater
deliberately does not cover any of them: a distro package is updated by the
distro's tooling, not by the app rewriting its own files in `/usr/bin`.

## Release procedure

One-time setup, then four steps per release.

### One-time

1. **Updater key.** Generated already; the public half is in `tauri.conf.json`.
   The private key is at `~/.config/reforgermods-launcher/updater.key` on the
   machine it was generated on and **is not in this repository**. Add it to CI:

   - `TAURI_SIGNING_PRIVATE_KEY` — the file's contents
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — empty; the key was generated without
     one, on the grounds that the key is already a secret in the same store and a
     password beside it adds a second secret rather than a second factor.
     Regenerate with a password if you disagree; both have to change together.

   Back it up. Losing it means every installed launcher stops accepting updates,
   and the only fix is a new key in a new release that everyone installs by hand.

2. **R2.** See [R2 setup](#r2-setup).

3. **Signing.** See [What is still outstanding](#what-is-still-outstanding).

### Per release

```sh
scripts/version.sh set 0.1.1     # rewrites all four version copies
# update the changelog in reforgermods-web: src/app/launcher/changelog/page.tsx
git commit -am "release 0.1.1" && git tag v0.1.1 && git push --follow-tags
```

The tag starts the pipeline. When it finishes, take `releases.json` from the run
artifacts, drop it into `public/static/launcher/releases.json` in
`reforgermods-web`, and deploy — until then the page still offers the previous
release.

### Building artifacts by hand

The release job is the supported path; these are for testing what it will
produce. Neither can Authenticode-sign, because that needs a Windows host.

```sh
scripts/build-windows.sh --release   # the bare .exe
scripts/build-arch.sh                # target/arch/*.pkg.tar.zst
```

A full Windows bundle can be cross-built on Linux, but Tauri calls this
experimental and it needs NSIS present:

```sh
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.config/reforgermods-launcher/updater.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
npm run tauri -- build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis
```

Debian's `makensis` has `/usr/share/nsis` compiled in as its data directory and
the bundler does not pass `NSISDIR` through, so a rootless install needs a
wrapper on `PATH` that exports it before exec'ing the real binary. 0.1.0 was
built this way and its updater signature verifies against the shipped public key.

Then assemble and describe the result:

```sh
python3 scripts/release-manifest.py --version "$(scripts/version.sh show)" \
  --artifacts dist/release --base-url https://dl.reforgermods.net --out dist/release
```

It writes both manifests from one scan of one directory, so the version, the URLs
and the filenames cannot disagree between the download page and the updater. An
artifact with no sibling `.sig` is never offered to the updater: the client would
reject it, and every launch would check, fail and retry.

## R2 setup

One bucket, public over a custom domain:

```sh
npx wrangler r2 bucket create reforgermods-launcher-downloads
```

Then, in the Cloudflare dashboard, attach `dl.reforgermods.net` to the bucket
(R2, the bucket, Settings, Public access, Custom domain). The release job needs
`CLOUDFLARE_API_TOKEN` (Workers R2 Storage: Edit) and `CLOUDFLARE_ACCOUNT_ID` as
secrets on the `release` environment.

Layout:

```text
dl.reforgermods.net/
  0.1.0/…-setup.exe          installer, and the updater artifact
  0.1.0/…-setup.exe.sig      detached updater signature
  0.1.0/…-0.1.0.exe          portable
  0.1.0/…-x86_64.pkg.tar.zst Arch package
  0.1.0/SHA256SUMS
  updates/latest.json        what the updater polls
  releases.json              published for reference; the site reads its own copy
```

Everything under a version prefix is immutable. Only the two manifests move.

## What is still outstanding

**Authenticode signing.** The pipeline is wired and inert. To turn it on:

1. Get a **verified Trusted Signing identity** for `cedarline.digital` and an
   Azure subscription that owns it. This is the blocking item — it is an identity
   verification, not a purchase, and it takes as long as it takes.
2. Create a Trusted Signing account and certificate profile.
3. Add to the `release` environment — secrets `AZURE_CLIENT_ID`,
   `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`; variables `RFM_SIGN_ENDPOINT`,
   `RFM_SIGN_ACCOUNT`, `RFM_SIGN_PROFILE`.

The job picks it up on the next tag with no code change. `packaging/windows/sign.cmd`
has never run on a Windows host, so budget one throwaway tag to shake it out
rather than discovering it on a release you meant to publish.

Until then the beta is unsigned and every download shows **"Windows protected
your PC"**. The download page says so plainly rather than letting people discover
it at the SmartScreen dialog — an honest note costs less trust than a surprise.
