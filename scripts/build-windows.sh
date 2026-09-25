#!/usr/bin/env bash
# Cross-compile a Windows executable from Linux — no Visual Studio, no root.
#
#   scripts/build-windows.sh [--debug]
#
# Produces target/x86_64-pc-windows-msvc/<profile>/reforgermods-launcher.exe,
# a standalone binary (the frontend is embedded at compile time). It needs the
# WebView2 runtime on the target machine, which ships with current Windows.
#
# Release is the default, and for a distributable binary it is the only correct
# choice: tauri-build sets `cfg(dev)` for debug profiles, and a `dev` build
# loads the UI from the Vite dev server on localhost:1420 rather than from the
# embedded assets. Run a debug build on a machine without `npm run dev` going
# and you get a blank window. `--debug` exists for attaching a debugger, and
# the script checks the result and says so.
#
# How it works: cargo-xwin downloads the MSVC CRT and Windows SDK from
# Microsoft into ~/.cache/cargo-xwin (~1.5 GB, once), and LLVM's clang-cl and
# lld-link stand in for cl.exe and link.exe. The LLVM pieces are fetched as
# Debian packages and unpacked into a local prefix, so nothing is installed
# system-wide and no sudo is required.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOLS="${RFM_WIN_TOOLS:-$HOME/.cache/rfm-win-tools}"
LLVM_VERSION="${RFM_LLVM_VERSION:-19}"
PROFILE_ARGS=(--release)
PROFILE_DIR="release"
if [[ "${1:-}" == "--debug" ]]; then
  PROFILE_ARGS=()
  PROFILE_DIR="debug"
fi

PREFIX="$TOOLS/root/usr/lib/llvm-$LLVM_VERSION/bin"

# 1. LLVM cross-linking tools, unpacked locally.
if [[ ! -x "$PREFIX/lld-link" || ! -e "$PREFIX/clang-cl" ]]; then
  echo "==> fetching LLVM $LLVM_VERSION tools into $TOOLS"
  mkdir -p "$TOOLS"
  (
    cd "$TOOLS"
    # clang provides clang-cl; lld provides lld-link; libclang-cpp is clang's runtime.
    apt-get download "clang-$LLVM_VERSION" "lld-$LLVM_VERSION" "libclang-cpp$LLVM_VERSION"
    for deb in *.deb; do dpkg-deb -x "$deb" ./root; done
  )
  # clang-cl is a driver mode of clang, not always shipped as its own file.
  [[ -e "$PREFIX/clang-cl" ]] || ln -s clang "$PREFIX/clang-cl"
fi

export PATH="$PREFIX:/usr/lib/llvm-$LLVM_VERSION/bin:$PATH"
export LD_LIBRARY_PATH="$TOOLS/root/usr/lib/llvm-$LLVM_VERSION/lib:${LD_LIBRARY_PATH:-}"

for tool in clang-cl lld-link llvm-rc llvm-lib; do
  command -v "$tool" >/dev/null || { echo "missing $tool" >&2; exit 1; }
done

# 2. cargo-xwin, which supplies the MSVC CRT and Windows SDK.
command -v cargo-xwin >/dev/null || cargo install cargo-xwin --locked

# cargo-xwin keeps its own clang-cl symlink and refuses to start if a stale one
# points somewhere else (for example a previous checkout's tools prefix).
XWIN_LINK="${XWIN_CACHE_DIR:-$HOME/.cache/cargo-xwin}/clang-cl"
if [[ -L "$XWIN_LINK" && "$(readlink -f "$XWIN_LINK")" != "$(readlink -f "$PREFIX/clang-cl")" ]]; then
  rm -f "$XWIN_LINK"
fi

# 3. The frontend, embedded into the binary by tauri-build.
echo "==> building frontend"
cd "$REPO"
npm run build

echo "==> cross-compiling for x86_64-pc-windows-msvc"
# --features custom-protocol is what makes this a production build; see the
# comment on that feature in src-tauri/Cargo.toml.
cargo xwin build --target x86_64-pc-windows-msvc -p reforgermods-launcher \
  --features custom-protocol "${PROFILE_ARGS[@]}"

EXE="$REPO/target/x86_64-pc-windows-msvc/$PROFILE_DIR/reforgermods-launcher.exe"
echo
echo "built: $EXE"
ls -lh "$EXE"

# Assert the binary is not a dev build.
#
# You cannot check this by grepping the exe: Tauri brotli-compresses the
# embedded assets, and it embeds the whole config (devUrl included) either way.
# The signal is `cargo:rustc-cfg=dev` from tauri-build, which cargo records in
# the build script's output file. A dev build looks completely fine here and
# fails with ERR_CONNECTION_REFUSED on the target machine, so it is worth
# asserting rather than assuming.
BUILD_OUT=$(ls -t "$REPO/target/x86_64-pc-windows-msvc/$PROFILE_DIR"/build/reforgermods-launcher-*/output 2>/dev/null | head -1)
if [[ -z "$BUILD_OUT" ]]; then
  echo "WARNING: could not find the build script output to verify the build mode." >&2
elif grep -q "rustc-cfg=dev" "$BUILD_OUT"; then
  echo
  echo "ERROR: this is a DEV build. It loads its UI from the Vite dev server on" >&2
  echo "localhost:1420 and will show ERR_CONNECTION_REFUSED anywhere else." >&2
  echo "The --features custom-protocol flag did not take effect." >&2
  exit 1
else
  echo "build mode: production (frontend embedded, no dev-server dependency)"
fi
