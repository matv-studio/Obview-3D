#!/usr/bin/env bash
# Construit OpenCASCADE (OCCT) en bibliothèques statiques, seulement les boîtes à outils utiles à Obview 3D
# (lecture STEP / IGES avec assemblages, noms et couleurs par XCAF, maillage) : D-27.
#
#   tools/occt.sh <dossier d'installation>
#
# Variables : OCCT_VERSION (étiquette git, V8_0_1 par défaut), OCCT_SRC (sources déjà présentes),
# OCCT_JOBS (tâches parallèles). Le dossier d'installation contient ensuite include/opencascade et lib/.
set -euo pipefail

prefix="${1:?dossier cible attendu}"
version="${OCCT_VERSION:-V8_0_1}"
work="${OCCT_WORK:-${prefix}-work}"
src="${OCCT_SRC:-$work/src}"
jobs="${OCCT_JOBS:-$(nproc 2>/dev/null || echo 2)}"

if [ -f "$prefix/.obview-occt-$version" ]; then
  echo "OCCT $version déjà construit dans $prefix"
  exit 0
fi

if [ ! -f "$src/CMakeLists.txt" ]; then
  git clone -q --depth 1 --branch "$version" https://github.com/Open-Cascade-SAS/OCCT.git "$src"
fi

# Fermeture des dépendances de TKDESTEP, TKDEIGES, TKXCAF, TKMesh (fichiers EXTERNLIB des sources).
toolkits="TKernel TKMath TKG2d TKG3d TKGeomBase TKBRep TKGeomAlgo TKTopAlgo TKPrim TKBO TKBool TKHLR TKShHealing \
TKMesh TKService TKV3d TKCDF TKLCAF TKCAF TKVCAF TKDE TKXSBase TKDESTEP TKDEIGES TKXCAF"

generator=()
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    generator=(-G "Visual Studio 17 2022" -A x64)
    # CMake pour Windows attend des chemins Windows.
    prefix="$(cygpath -m "$prefix")"; src="$(cygpath -m "$src")"; work="$(cygpath -m "$work")"
    ;;
  *) if command -v ninja >/dev/null; then generator=(-G Ninja); fi ;;
esac

cmake "${generator[@]}" -S "$src" -B "$work/build" \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_LIBRARY_TYPE=Static \
  -DINSTALL_DIR="$prefix" \
  -DINSTALL_DIR_LIB=lib \
  -DINSTALL_DIR_INCLUDE=include/opencascade \
  -DBUILD_MODULE_FoundationClasses=OFF \
  -DBUILD_MODULE_ModelingData=OFF \
  -DBUILD_MODULE_ModelingAlgorithms=OFF \
  -DBUILD_MODULE_Visualization=OFF \
  -DBUILD_MODULE_ApplicationFramework=OFF \
  -DBUILD_MODULE_DataExchange=OFF \
  -DBUILD_MODULE_Draw=OFF \
  -DBUILD_ADDITIONAL_TOOLKITS="$toolkits" \
  -DBUILD_DOC_Overview=OFF \
  -DBUILD_SAMPLES_QT=OFF \
  -DUSE_FREETYPE=OFF -DUSE_TK=OFF -DUSE_TCL=OFF -DUSE_XLIB=OFF -DUSE_OPENGL=OFF -DUSE_GLES2=OFF -DUSE_D3D=OFF \
  -DUSE_FREEIMAGE=OFF -DUSE_FFMPEG=OFF -DUSE_OPENVR=OFF -DUSE_RAPIDJSON=OFF -DUSE_DRACO=OFF -DUSE_TBB=OFF \
  -DUSE_VTK=OFF -DUSE_EIGEN=OFF \
  -DCMAKE_MSVC_RUNTIME_LIBRARY="MultiThreaded\$<\$<CONFIG:Debug>:Debug>DLL"

cmake --build "$work/build" --config Release --parallel "$jobs"
cmake --install "$work/build" --config Release
mkdir -p "$prefix/licence"
cp "$src/LICENSE_LGPL_21.txt" "$src/OCCT_LGPL_EXCEPTION.txt" "$prefix/licence/"
touch "$prefix/.obview-occt-$version"
echo "OCCT $version installé dans $prefix"
