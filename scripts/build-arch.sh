#!/usr/bin/env bash
# Build a native Arch Linux package (.pkg.tar.zst) from this checkout.
#
#   scripts/build-arch.sh [--no-check]
#
# The build runs inside an `archlinux:base-devel` container, so the binary is
# linked against Arch's own glibc and WebKit rather than this machine's. That
# matters: a Tauri app built on Debian links libwebkit2gtk-4.1 with Debian's
# soname and ABI, and handing that to an Arch user is how you get a blank
# window or a missing-symbol crash. Building on Arch removes the question.
#
# Output: target/arch/reforgermods-launcher-<version>-1-x86_64.pkg.tar.zst,
# installed on the target machine with `sudo pacman -U <file>`. pacman resolves
# webkit2gtk-4.1 and the rest, so there is nothing else to install by hand.
#
# Docker is only a clean Arch userspace here — nothing is deployed and no
# daemon is left running. --no-check skips the rfm-core test suite.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="${RFM_ARCH_IMAGE:-rfm-arch-build}"
PKGBUILD="$REPO/packaging/arch/PKGBUILD"
STAGE="$REPO/target/arch"
MAKEPKG_ARGS=(--force --noconfirm --clean)

if [[ "${1:-}" == "--no-check" ]]; then
  MAKEPKG_ARGS+=(--nocheck)
fi

command -v docker >/dev/null || { echo "docker is required" >&2; exit 1; }
docker info >/dev/null 2>&1 || { echo "the docker daemon is not reachable" >&2; exit 1; }

# 1. One version, agreed in both places.
#
# tauri.conf.json is what the app reports about itself, so it is the source of
# truth; the PKGBUILD carries its own copy because makepkg needs it literally.
# A silent disagreement here ships a package whose pacman version does not
# match the version in the window, so it is checked rather than assumed.
CONF_VERSION="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$REPO/src-tauri/tauri.conf.json")"
PKG_VERSION="$(sed -n 's/^pkgver=\(.*\)$/\1/p' "$PKGBUILD")"
if [[ -z "$CONF_VERSION" || -z "$PKG_VERSION" ]]; then
  echo "ERROR: could not read the version from tauri.conf.json or the PKGBUILD." >&2
  exit 1
fi
if [[ "$CONF_VERSION" != "$PKG_VERSION" ]]; then
  echo "ERROR: version mismatch — tauri.conf.json says $CONF_VERSION, PKGBUILD says $PKG_VERSION." >&2
  echo "Update pkgver in packaging/arch/PKGBUILD (and reset pkgrel to 1)." >&2
  exit 1
fi
VERSION="$CONF_VERSION"

# 2. The build image: Arch plus the toolchains, and a user whose uid matches
# this one so everything written to the bind mount stays owned by the caller.
if [[ -n "${RFM_ARCH_REBUILD_IMAGE:-}" ]] || ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  echo "==> building the $IMAGE container image"
  docker build -t "$IMAGE" \
    --build-arg "UID=$(id -u)" --build-arg "GID=$(id -g)" - <<'DOCKERFILE'
FROM archlinux:base-devel
ARG UID=1000
ARG GID=1000
RUN pacman -Syu --noconfirm --needed rust nodejs npm webkit2gtk-4.1 cmake \
 && pacman -Scc --noconfirm
RUN groupadd -g "$GID" builder 2>/dev/null || true \
 && useradd -u "$UID" -g "$GID" -m -d /home/builder builder 2>/dev/null || true \
 && mkdir -p /cache/cargo /cache/npm /build \
 && chown -R "$UID:$GID" /cache /build /home/builder
USER $UID:$GID
ENV CARGO_HOME=/cache/cargo npm_config_cache=/cache/npm
WORKDIR /build
DOCKERFILE
fi

# 3. The source tarball makepkg consumes: the working tree, minus everything
# that is generated or host-specific. target/ especially — those are Debian
# build artifacts and cargo would happily reuse the stale ones.
echo "==> staging sources for $VERSION"
rm -rf "$STAGE"
mkdir -p "$STAGE"
tar --create --gzip --file "$STAGE/reforgermods-launcher-$VERSION.tar.gz" \
  --directory "$REPO" \
  --transform "s,^\.,reforgermods-launcher-$VERSION," \
  --exclude=./.git \
  --exclude=./node_modules \
  --exclude=./target \
  --exclude=./dist \
  --exclude=./src-tauri/target \
  --exclude=./src-tauri/gen/schemas \
  .
cp "$PKGBUILD" "$STAGE/PKGBUILD"

# 4. Named volumes keep the cargo registry and npm cache across runs, so only
# the first build pays for downloading the dependency tree.
docker volume create rfm-arch-cargo >/dev/null
docker volume create rfm-arch-npm >/dev/null
docker run --rm --user root \
  -v rfm-arch-cargo:/cache/cargo -v rfm-arch-npm:/cache/npm \
  "$IMAGE" chown "$(id -u):$(id -g)" /cache/cargo /cache/npm

echo "==> running makepkg in $IMAGE"
docker run --rm \
  -v "$STAGE:/build" \
  -v rfm-arch-cargo:/cache/cargo \
  -v rfm-arch-npm:/cache/npm \
  "$IMAGE" makepkg "${MAKEPKG_ARGS[@]}"

PKG="$(ls -t "$STAGE"/*.pkg.tar.zst 2>/dev/null | head -1)"
if [[ -z "$PKG" ]]; then
  echo "ERROR: makepkg produced no package." >&2
  exit 1
fi

# 5. Checksum, for handing the file over.
#
# The package is already known to be a production build rather than a dev one:
# build() asserts that before packaging, which is the only point where the
# evidence still exists.
sha256sum "$PKG" | sed "s,$STAGE/,," > "$STAGE/SHA256SUMS"

echo
echo "built: $PKG"
ls -lh "$PKG"
echo
echo "install on Arch with:  sudo pacman -U $(basename "$PKG")"
