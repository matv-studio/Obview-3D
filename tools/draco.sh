#!/usr/bin/env bash
# Construit la bibliothèque Draco de Google (compression de maillages glTF, KHR_draco_mesh_compression) en
# bibliothèque statique : D-30. C'est le codec de référence, celui que Blender, three.js, glTFast et
# Babylon.js embarquent pour décoder.
#
#   tools/draco.sh <dossier d'installation>
#
# Variables : DRACO_VERSION (étiquette git, 1.5.9 par défaut), DRACO_JOBS (tâches parallèles). Le dossier
# d'installation contient ensuite include/draco et lib/ (draco.lib ou libdraco.a).
set -euo pipefail

prefix="${1:?dossier cible attendu}"
version="${DRACO_VERSION:-1.5.9}"
work="${DRACO_WORK:-${prefix}-work}"
src="$work/src"
jobs="${DRACO_JOBS:-$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2)}"

if [ -f "$prefix/.obview-draco-$version" ]; then
  echo "Draco $version déjà construit dans $prefix"
  exit 0
fi

if [ ! -f "$src/CMakeLists.txt" ]; then
  git clone -q --depth 1 --branch "$version" https://github.com/google/draco.git "$src"
fi

generator=()
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    generator=(-G "Visual Studio 17 2022" -A x64)
    prefix="$(cygpath -m "$prefix")"; src="$(cygpath -m "$src")"; work="$(cygpath -m "$work")"
    ;;
  *) if command -v ninja >/dev/null; then generator=(-G Ninja); fi ;;
esac

# Sans le transcodeur (qui tire Eigen, tinygltf et l'écriture glTF de Draco) : seuls l'encodeur et le
# décodeur de maillages servent, la structure glTF est écrite par obv-mesh.
cmake "${generator[@]}" -S "$src" -B "$work/build" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$prefix" \
  -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
  -DBUILD_SHARED_LIBS=OFF \
  -DDRACO_TRANSCODER_SUPPORTED=OFF \
  -DDRACO_TESTS=OFF \
  -DDRACO_JS_GLUE=OFF \
  -DDRACO_UNITY_PLUGIN=OFF \
  -DDRACO_MAYA_PLUGIN=OFF \
  -DCMAKE_MSVC_RUNTIME_LIBRARY="MultiThreaded\$<\$<CONFIG:Debug>:Debug>DLL"
cmake --build "$work/build" --config Release --parallel "$jobs"
cmake --install "$work/build" --config Release
cp "$src/LICENSE" "$prefix/LICENSE-draco"
touch "$prefix/.obview-draco-$version"
echo "Draco $version installé dans $prefix"
