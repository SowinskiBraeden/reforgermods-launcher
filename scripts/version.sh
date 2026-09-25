#!/usr/bin/env bash
# The launcher's version, in one place.
#
#   scripts/version.sh show            print the canonical version
#   scripts/version.sh check [tag]     fail if any copy disagrees
#   scripts/version.sh set 0.2.0       rewrite every copy
#
# The canonical value is `[workspace.package] version` in the root Cargo.toml,
# because that is the one the *running binary* reports: rfm-core builds its
# User-Agent from CARGO_PKG_VERSION, so the API's per-version rollups are keyed
# off it whatever the other files say.
#
# It has to be repeated in four places — Cargo's workspace manifest, package.json,
# tauri.conf.json and the PKGBUILD — because no two of those tools can read each
# other's format. tauri.conf.json could inherit from Cargo.toml by omitting the
# field, but the src-tauri crate declares `version.workspace = true` and Tauri
# parses that manifest itself rather than resolving workspace inheritance, so the
# value has to be written out. `check` is what makes the repetition safe.
#
# This matters more than tidiness: an updater whose manifest version disagrees
# with the installed version either loops forever or never fires
# (docs/distribution.md section 6).
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

canonical() {
  # The first `version = "x.y.z"` under [workspace.package].
  awk '/^\[workspace\.package\]/{f=1;next} /^\[/{f=0} f && /^version *=/{gsub(/[^0-9a-zA-Z.\-+]/,"",$3);print $3;exit}' Cargo.toml
}

read_package_json() { python3 -c 'import json;print(json.load(open("package.json"))["version"])'; }
read_tauri_conf()   { python3 -c 'import json;print(json.load(open("src-tauri/tauri.conf.json"))["version"])'; }
read_pkgbuild()     { sed -n 's/^pkgver=\(.*\)$/\1/p' packaging/arch/PKGBUILD; }

SEMVER='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'

cmd_show() { canonical; }

cmd_check() {
  local want tag="${1:-}" status=0
  want="$(canonical)"
  if [[ ! "$want" =~ $SEMVER ]]; then
    echo "Cargo.toml [workspace.package] version is not semver: '$want'" >&2
    return 1
  fi
  local name value
  for name in package.json src-tauri/tauri.conf.json packaging/arch/PKGBUILD; do
    case "$name" in
      package.json) value="$(read_package_json)" ;;
      src-tauri/tauri.conf.json) value="$(read_tauri_conf)" ;;
      packaging/arch/PKGBUILD) value="$(read_pkgbuild)" ;;
    esac
    if [[ "$value" != "$want" ]]; then
      echo "$name: $value, want $want (run: scripts/version.sh set $want)" >&2
      status=1
    fi
  done
  # A tag is checked only when one is given, so the same command serves CI on a
  # tag push and a developer on a branch. Both `1.2.3` and `v1.2.3` are accepted.
  if [[ -n "$tag" && "${tag#v}" != "$want" ]]; then
    echo "tag $tag does not match version $want" >&2
    status=1
  fi
  if [[ $status -eq 0 ]]; then
    echo "version $want is consistent${tag:+ with tag $tag}"
  fi
  return $status
}

cmd_set() {
  local want="${1:?usage: scripts/version.sh set <version>}"
  want="${want#v}"
  if [[ ! "$want" =~ $SEMVER ]]; then
    echo "not a semver version: '$want'" >&2
    return 1
  fi
  python3 - "$want" <<'PY'
import io, json, re, sys
want = sys.argv[1]

# Cargo.toml: only the [workspace.package] version, never a dependency's.
path = "Cargo.toml"
text = io.open(path, encoding="utf-8").read()
def bump_workspace_version(match):
    return match.group(1) + '"' + want + '"'
new, count = re.subn(r'(\[workspace\.package\]\n(?:[^\[]*?\n)?version *= *)"[^"]*"', bump_workspace_version, text)
assert count == 1, f"{path}: matched {count} workspace versions, expected 1"
io.open(path, "w", encoding="utf-8").write(new)

# package.json / tauri.conf.json: rewrite the single field, preserving formatting
# (a json.dump round trip would reformat files a human maintains).
for path, key in (("package.json", "version"), ("src-tauri/tauri.conf.json", "version")):
    text = io.open(path, encoding="utf-8").read()
    new, count = re.subn(r'("%s"\s*:\s*)"[^"]*"' % key, lambda m: m.group(1) + '"' + want + '"', text, count=1)
    assert count == 1, f"{path}: no {key} field"
    json.loads(new)
    io.open(path, "w", encoding="utf-8").write(new)

path = "packaging/arch/PKGBUILD"
text = io.open(path, encoding="utf-8").read()
new, count = re.subn(r'(?m)^pkgver=.*$', "pkgver=" + want, text, count=1)
assert count == 1, f"{path}: no pkgver"
io.open(path, "w", encoding="utf-8").write(new)
print("set version", want)
PY
  cmd_check
}

case "${1:-show}" in
  show) cmd_show ;;
  check) shift; cmd_check "$@" ;;
  set) shift; cmd_set "$@" ;;
  *) echo "usage: scripts/version.sh [show|check [tag]|set <version>]" >&2; exit 2 ;;
esac
