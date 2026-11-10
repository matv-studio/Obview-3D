#!/usr/bin/env bash
# Installe le SDK Vulkan de LunarG pour macOS sans fenêtre ni droits d'administrateur (copy_only=1) :
# chargeur Vulkan, pilote KosmicKrisp, glslangValidator, vulkaninfo (D-30).
#
#   tools/macos-sdk.sh <version, ex. 1.4.363.0> <dossier d'installation>
#
# Le dossier contient ensuite macOS/lib, macOS/bin et macOS/share/vulkan/icd.d (documentation macOS du SDK).
set -euo pipefail
# Compatible avec le bash 3.2 de macOS : pas d'apostrophe dans les ${…:?message}.

version="${1:?version attendue}"
root="${2:?dossier cible attendu}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

curl -fsSL -o "$work/sdk.zip" "https://sdk.lunarg.com/sdk/download/$version/mac/vulkansdk-macos-$version.zip"
ditto -x -k "$work/sdk.zip" "$work/x"
installer="$(find "$work/x" -maxdepth 2 -name '*.app' -type d | head -1)"
[ -n "$installer" ] || { echo "installeur introuvable dans l'archive du SDK" >&2; exit 1; }
name="$(basename "$installer" .app)"
"$installer/Contents/MacOS/$name" --root "$root" --accept-licenses --default-answer --confirm-command \
  install copy_only=1
ls "$root/macOS/lib" "$root/macOS/share/vulkan/icd.d"
test -x "$root/macOS/bin/glslangValidator"
