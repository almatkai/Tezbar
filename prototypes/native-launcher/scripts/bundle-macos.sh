#!/bin/bash
# Local preview only. Read docs/MACOS_SIGNING.md before invoking.
set -euo pipefail
if [[ ($# != 2 && $# != 4) || $1 != --local-preview || ($# == 4 && $3 != --binary) ]]; then
  echo 'usage: bundle-macos.sh --local-preview <marked OS-temp fixture root> [--binary <path>]' >&2; exit 2
fi
prototype_dir="$(cd "$(dirname "$0")/.." && pwd -P)"
prototype_root="$(/opt/homebrew/bin/python3 - "$2" <<'PY'
import pathlib, sys, tempfile
p=pathlib.Path(sys.argv[1]); r=p.resolve(strict=True)
assert p.is_absolute() and p==r, 'use a canonical absolute root'
assert pathlib.Path(tempfile.gettempdir()).resolve() in r.parents, 'OS temp only'
assert (r/'.tezbar-native-launcher-prototype').is_file(), 'missing fixture marker'
assert not (r/'Tezbar Launcher Prototype.app').exists(), 'refusing existing app'
print(r)
PY
)"
prototype_app="$prototype_root/Tezbar Launcher Prototype.app"
mkdir -p "$prototype_app/Contents/MacOS" "$prototype_app/Contents/Resources"
cp "$prototype_dir/macos/Info.plist" "$prototype_app/Contents/Info.plist"
cp "${4:-$prototype_dir/target/release/tezbar-launcher-prototype}" "$prototype_app/Contents/MacOS/tezbar-launcher-prototype"
codesign --force --sign - --identifier com.tezbar.prototype.nativelauncher "$prototype_app"
codesign --verify --strict "$prototype_app"
printf '%s\n' "$prototype_app"
