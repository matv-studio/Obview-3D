#!/usr/bin/env bash
# SDK Vulkan de LunarG pour Linux, pour son glslangValidator récent (mesh shaders) sur une distribution ancienne
# (Ubuntu 22.04, construction des paquets : D-32).
#
#   tools/linux-sdk.sh <version, ex. 1.4.363.0> <dossier d'installation>
#
# VULKAN_SDK vaut ensuite <dossier>/<version>/x86_64.
set -euo pipefail

version="${1:?version attendue}"
root="${2:?dossier cible attendu}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$root"
for ext in tar.xz tar.gz; do
  if curl -fsSL -o "$work/sdk.$ext" "https://sdk.lunarg.com/sdk/download/$version/linux/vulkansdk-linux-x86_64-$version.$ext"; then
    tar -xf "$work/sdk.$ext" -C "$root"
    break
  fi
done
"$root/$version/x86_64/bin/glslangValidator" --version | head -1
