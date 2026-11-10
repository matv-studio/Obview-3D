#!/usr/bin/env bash
# Assemble « Obview 3D.app » pour macOS sur Apple Silicon, avec le chargeur Vulkan et le pilote KosmicKrisp
# (Vulkan 1.4 sur Metal 4, macOS 26 et plus) tirés du SDK Vulkan de LunarG : D-30.
#
#   tools/macos-app.sh <exécutable> <version> <SDK Vulkan, dossier macOS> <dossier de sortie>
#
# Disposition (documentation macOS du SDK) : chargeur et pilote dans Contents/Frameworks, manifeste du pilote
# dans Contents/Resources/vulkan/icd.d, que le chargeur lit en premier pour une application empaquetée.
# Sans identité Apple : signature ad hoc (D-30), à ouvrir une première fois par « Ouvrir quand même ».
set -euo pipefail

exe="${1:?exécutable attendu}"
version="${2:?version attendue}"
sdk="${3:?dossier macOS du SDK Vulkan attendu}"
out="${4:?dossier de sortie attendu}"
here="$(cd "$(dirname "$0")/.." && pwd)"

app="$out/Obview 3D.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Frameworks" "$app/Contents/Resources/vulkan/icd.d" \
  "$app/Contents/Resources/licences"
cp "$exe" "$app/Contents/MacOS/obview"
cp "$here/apps/obview/assets/obview.icns" "$app/Contents/Resources/obview.icns"

# Pilote : la bibliothèque que désigne le manifeste du SDK, puis un manifeste qui pointe vers Frameworks.
icd="$sdk/share/vulkan/icd.d/libkosmickrisp_icd.json"
[ -f "$icd" ] || { echo "manifeste KosmicKrisp absent : $icd" >&2; exit 1; }
driver="$(python3 - "$icd" "$sdk" <<'PY'
import json, os, sys
icd, sdk = sys.argv[1], sys.argv[2]
p = json.load(open(icd))["ICD"]["library_path"]
if "/" not in p:
    p = os.path.join(sdk, "lib", p)
elif not os.path.isabs(p):
    p = os.path.normpath(os.path.join(os.path.dirname(icd), p))
print(p)
PY
)"
[ -f "$driver" ] || { echo "pilote KosmicKrisp absent : $driver" >&2; exit 1; }
cp -L "$driver" "$app/Contents/Frameworks/libvulkan_kosmickrisp.dylib"
python3 - "$icd" "$app/Contents/Resources/vulkan/icd.d/libkosmickrisp_icd.json" <<'PY'
import json, sys
m = json.load(open(sys.argv[1]))
m["ICD"]["library_path"] = "../../../Frameworks/libvulkan_kosmickrisp.dylib"
json.dump(m, open(sys.argv[2], "w"), indent=4)
PY
cp -L "$sdk/lib/libvulkan.1.dylib" "$app/Contents/Frameworks/libvulkan.1.dylib"

# Aucune dépendance hors du système : sinon l'application ne se lancerait que sur la machine de construction.
for lib in "$app/Contents/Frameworks/"*.dylib "$app/Contents/MacOS/obview"; do
  otool -L "$lib" | tail -n +2 | awk '{print $1}' | while read -r dep; do
    case "$dep" in
      /usr/lib/*|/System/*|@rpath/libvulkan*|@loader_path/*|@executable_path/*) ;;
      *) if [ "$(basename "$dep")" != "$(basename "$lib")" ]; then
           echo "dépendance hors système dans $(basename "$lib") : $dep" >&2; exit 1
         fi ;;
    esac
  done
done

cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Obview 3D</string>
  <key>CFBundleDisplayName</key><string>Obview 3D</string>
  <key>CFBundleIdentifier</key><string>io.github.matv-studio.obview3d</string>
  <key>CFBundleExecutable</key><string>obview</string>
  <key>CFBundleIconFile</key><string>obview</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>LSMinimumSystemVersion</key><string>26.0</string>
  <key>LSRequiresNativeExecution</key><true/>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
EOF

# Licences : polices, OpenCASCADE, Draco, chargeur Vulkan (Apache 2.0), Mesa / KosmicKrisp (MIT).
lic="$app/Contents/Resources/licences"
cp "$here/crates/obv-ui/assets/fonts/OFL-geist.txt" "$lic/Geist-OFL.txt"
[ -n "${OCCT_DIR:-}" ] && cp "$OCCT_DIR"/licence/* "$lic/"
[ -n "${DRACO_DIR:-}" ] && cp "$DRACO_DIR/LICENSE-draco" "$lic/Draco-Apache-2.0.txt"
curl -fsSL -o "$lic/Vulkan-Loader-Apache-2.0.txt" \
  https://raw.githubusercontent.com/KhronosGroup/Vulkan-Loader/main/LICENSE.txt
curl -fsSL -o "$lic/Mesa-KosmicKrisp-MIT.rst" https://gitlab.freedesktop.org/mesa/mesa/-/raw/main/docs/license.rst

codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"
echo "$app prêt"
