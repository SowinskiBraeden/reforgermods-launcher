#!/usr/bin/env python3
"""Build the two release manifests from a directory of artifacts.

    scripts/release-manifest.py --version 0.1.0 --artifacts dist/release \
        --base-url https://dl.reforgermods.net --out dist/release

Writes two files, because two different consumers need two different shapes and
conflating them is how a download page and an updater drift apart:

  releases.json  what the website's /launcher/ page reads. Every download it
                 should offer, with size and SHA-256 so a visitor can verify.
                 Consumed by public/static/launcher.js in reforgermods-web.

  latest.json    what tauri-plugin-updater polls. Only the platforms that have
                 an updater artifact and a signature, in Tauri's own schema.

Both are generated from the same scan of the same directory, so the version, the
URLs and the filenames cannot disagree between them.

Artifacts are recognised by suffix rather than listed, so a build that fails to
produce one is a manifest that is missing it — not a manifest with a dead link.
"""

import argparse
import hashlib
import json
import pathlib
import sys
from datetime import datetime, timezone

# Suffix -> (platform id understood by launcher.js, kind, human label).
# Order matters: the longest, most specific suffix has to be tested first, or
# "-setup.exe" is matched by the portable ".exe" rule.
ARTIFACT_RULES = [
    (".nsis.zip", "windows", "updater", "Updater package"),
    ("-setup.exe", "windows", "installer", "Installer, Windows 10 or 11 64-bit"),
    # An NSIS bundle is signed in place: the updater downloads the same
    # -setup.exe a visitor does, so that file is both a download and the updater
    # artifact. Any artifact with a sibling .sig is treated as one, below.
    (".pkg.tar.zst", "arch", "pacman", "Arch Linux package"),
    (".deb", "debian", "deb", "Debian / Ubuntu package"),
    (".AppImage", "appimage", "appimage", "AppImage, no install"),
    (".rpm", "fedora", "rpm", "Fedora package"),
    (".exe", "windows", "portable", "Portable, no installer"),
]

# The updater target triple Tauri looks for in latest.json.
UPDATER_TARGETS = {"windows": "windows-x86_64"}


def classify(name):
    for suffix, platform, kind, label in ARTIFACT_RULES:
        if name.endswith(suffix):
            return platform, kind, label
    return None


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--artifacts", required=True, type=pathlib.Path)
    parser.add_argument("--base-url", required=True, help="e.g. https://dl.reforgermods.net")
    parser.add_argument("--out", required=True, type=pathlib.Path)
    parser.add_argument("--notes-url", default="https://reforgermods.net/launcher/changelog/")
    parser.add_argument(
        "--notes",
        default="",
        help="Release notes shown by the updater. Kept short: it is a dialog, not a changelog.",
    )
    parser.add_argument(
        "--published-at",
        default=None,
        help="RFC3339 timestamp. Defaults to now, in UTC.",
    )
    args = parser.parse_args()

    version = args.version.lstrip("v")
    base = args.base_url.rstrip("/")
    prefix = f"{base}/{version}"
    published = args.published_at or datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")

    if not args.artifacts.is_dir():
        sys.exit(f"not a directory: {args.artifacts}")

    downloads, updater_platforms, skipped = [], {}, []
    # Signatures sit beside their artifact as <artifact>.sig; they are read, never
    # listed as downloads.
    for path in sorted(p for p in args.artifacts.iterdir() if p.is_file()):
        if path.suffix == ".sig" or path.name in ("releases.json", "latest.json", "SHA256SUMS"):
            continue
        classified = classify(path.name)
        if classified is None:
            skipped.append(path.name)
            continue
        platform, kind, label = classified
        url = f"{prefix}/{path.name}"

        # A sibling .sig makes this the platform's updater artifact, whether or
        # not it is also a download. An unsigned one is never offered: the client
        # rejects it, so publishing it means every launch checks, fails, retries.
        signature = path.with_name(path.name + ".sig")
        target = UPDATER_TARGETS.get(platform)
        if signature.is_file():
            if target is None:
                skipped.append(f"{path.name} (signed, but no updater target for {platform})")
            elif target in updater_platforms:
                skipped.append(f"{path.name} (a {target} updater artifact was already chosen)")
            else:
                updater_platforms[target] = {
                    "signature": signature.read_text(encoding="utf-8").strip(),
                    "url": url,
                }
        elif kind == "updater":
            skipped.append(f"{path.name} (no {signature.name}; not offered to the updater)")

        if kind == "updater":
            # A dedicated updater package is not a human download.
            continue

        downloads.append(
            {
                "platform": platform,
                "kind": kind,
                "label": label,
                "url": url,
                "size": path.stat().st_size,
                "sha256": sha256(path),
            }
        )

    # launcher.js offers the first entry whose platform matches as the primary
    # download, so an installer has to come before a portable build of the same
    # platform. Within a platform, installer first; otherwise by platform id for
    # a stable file.
    kind_order = {"installer": 0, "pacman": 0, "deb": 0, "rpm": 0, "appimage": 1, "portable": 2}
    # Windows leads because that is where the players are; the rest follow in the
    # order the download table should read.
    platform_order = {"windows": 0, "debian": 1, "arch": 2, "fedora": 3, "appimage": 4}
    downloads.sort(key=lambda d: (platform_order.get(d["platform"], 9), kind_order.get(d["kind"], 9)))

    args.out.mkdir(parents=True, exist_ok=True)

    releases = {
        "version": version,
        "publishedAt": published,
        "notesUrl": args.notes_url,
        "downloads": downloads,
    }
    (args.out / "releases.json").write_text(json.dumps(releases, indent=2) + "\n", encoding="utf-8")

    latest = {
        "version": version,
        "notes": args.notes or f"reforgermods.net launcher {version}",
        "pub_date": published,
        "platforms": updater_platforms,
    }
    (args.out / "latest.json").write_text(json.dumps(latest, indent=2) + "\n", encoding="utf-8")

    print(f"version {version}, published {published}")
    for download in downloads:
        print(f"  download  {download['platform']:9} {download['kind']:9} {download['size']:>10,} B  {download['url']}")
    for target, entry in updater_platforms.items():
        print(f"  updater   {target}  {entry['url']}")
    for name in skipped:
        print(f"  skipped   {name}")
    if not downloads:
        sys.exit("no downloadable artifacts were found: refusing to write an empty manifest")
    if not updater_platforms:
        # Not fatal: a Linux-only or unsigned build is a legitimate state. It is
        # called out because it means the updater will find nothing.
        print("  WARNING: latest.json has no platforms; the updater will never fire")


if __name__ == "__main__":
    main()
