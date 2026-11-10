#!/usr/bin/env bash
# Image disque d'installation macOS (D-32) : « Obview 3D.app » et un raccourci vers Applications, à glisser.
#
#   tools/macos-dmg.sh <Obview 3D.app> <fichier .dmg>
#
# Compatible avec le bash 3.2 de macOS (pas d'apostrophe dans les ${…:?message}).
set -euo pipefail

app="${1:?paquet .app attendu}"
out="${2:?fichier dmg attendu}"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

ditto "$app" "$stage/$(basename "$app")"
ln -s /Applications "$stage/Applications"
rm -f "$out"
hdiutil create -volname "Obview 3D" -srcfolder "$stage" -fs HFS+ -format UDZO -imagekey zlib-level=9 -ov "$out"
hdiutil verify "$out"
echo "$out prêt"
