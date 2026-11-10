#!/usr/bin/env bash
# Paquets Linux d'Obview 3D (D-32) : .deb (Debian, Ubuntu et dérivés ; menu des applications, « Ouvrir avec »,
# mise à jour depuis l'application par pkexec) et AppImage (toutes distributions, fichier unique remplacé en
# place par la mise à jour).
#
#   tools/linux-package.sh <exécutable> <version> <dossier des licences> <dossier de sortie>
#
# Construire sur la plus ancienne distribution visée (glibc) : la CI construit sur Ubuntu 22.04.
set -euo pipefail

exe="${1:?exécutable attendu}"
version="${2:?version attendue}"
licences="${3:?dossier des licences attendu}"
out="${4:?dossier de sortie attendu}"
here="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"

# Types reconnus par shared-mime-info, plus ceux que déclare packaging/linux/obview-3d.xml.
mimes="model/step;model/iges;model/stl;model/obj;model/gltf-binary;model/gltf+json;application/x-fbx;"

desktop() {
  cat <<EOF
[Desktop Entry]
Type=Application
Name=Obview 3D
GenericName=3D viewer
GenericName[fr]=Visionneuse 3D
Comment=View CAD files and meshes, convert to optimized meshes
Comment[fr]=Visionner les fichiers CAO et les maillages, convertir en maillages optimisés
Exec=$1 %f
Icon=obview-3d
Terminal=false
Categories=Graphics;3DGraphics;Viewer;Engineering;
MimeType=$mimes
Keywords=STEP;IGES;STL;OBJ;glTF;GLB;FBX;CAD;
StartupWMClass=obview
EOF
}

# ---- .deb ----
root="$work/deb"
mkdir -p "$root/DEBIAN" "$root/opt/obview-3d/licences" "$root/usr/bin" "$root/usr/share/applications" \
  "$root/usr/share/icons/hicolor/256x256/apps" "$root/usr/share/icons/hicolor/scalable/apps" \
  "$root/usr/share/mime/packages"
install -m 755 "$exe" "$root/opt/obview-3d/obview-3d"
cp -r "$licences"/. "$root/opt/obview-3d/licences/"
ln -s /opt/obview-3d/obview-3d "$root/usr/bin/obview-3d"
desktop obview-3d > "$root/usr/share/applications/obview-3d.desktop"
install -m 644 "$here/apps/obview/assets/obview-256.png" "$root/usr/share/icons/hicolor/256x256/apps/obview-3d.png"
install -m 644 "$here/apps/obview/assets/obview.svg" "$root/usr/share/icons/hicolor/scalable/apps/obview-3d.svg"
install -m 644 "$here/packaging/linux/obview-3d.xml" "$root/usr/share/mime/packages/obview-3d.xml"

# Dépendances des bibliothèques liées (dpkg-shlibdeps), plus celles chargées à l'exécution : Vulkan, EGL.
mkdir -p "$work/shlib/debian"
echo "Source: obview-3d" > "$work/shlib/debian/control"
deps="$(cd "$work/shlib" && dpkg-shlibdeps -O "$root/opt/obview-3d/obview-3d" 2>/dev/null | sed -n 's/^shlibs:Depends=//p')"
size="$(du -sk "$root" | cut -f1)"
cat > "$root/DEBIAN/control" <<EOF
Package: obview-3d
Version: $version
Architecture: amd64
Section: graphics
Priority: optional
Installed-Size: $size
Depends: ${deps:+$deps, }libvulkan1
Recommends: mesa-vulkan-drivers | vulkan-icd, libegl1, xdg-desktop-portal
Maintainer: matv-studio <noreply@github.com>
Homepage: https://github.com/matv-studio/Obview-3D
Description: Fast desktop 3D viewer for CAD files and meshes
 STEP, IGES, STL, OBJ, glTF / GLB and FBX viewer, with conversion to
 optimized meshes (GLB, OBJ, FBX).
EOF
deb="$out/obview-3d_${version}_amd64.deb"
dpkg-deb --root-owner-group -Zxz --build "$root" "$deb"
echo "$deb prêt"

# ---- AppImage ---- (APPIMAGETOOL=none : .deb seul)
if [ "${APPIMAGETOOL:-}" = "none" ]; then exit 0; fi
appdir="$work/Obview3D.AppDir"
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/licences" "$appdir/usr/share/icons/hicolor/256x256/apps"
install -m 755 "$exe" "$appdir/usr/bin/obview-3d"
cp -r "$licences"/. "$appdir/usr/share/licences/"
desktop obview-3d > "$appdir/obview-3d.desktop"
install -m 644 "$here/apps/obview/assets/obview-256.png" "$appdir/obview-3d.png"
install -m 644 "$here/apps/obview/assets/obview-256.png" "$appdir/usr/share/icons/hicolor/256x256/apps/obview-3d.png"
cat > "$appdir/AppRun" <<'EOF'
#!/bin/sh
here="$(dirname "$(readlink -f "$0")")"
exec "$here/usr/bin/obview-3d" "$@"
EOF
chmod 755 "$appdir/AppRun"
tool="${APPIMAGETOOL:-$work/appimagetool}"
if [ ! -x "$tool" ]; then
  curl -fsSL -o "$tool" https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod 755 "$tool"
fi
image="$out/Obview-3D-${version}-linux-x86_64.AppImage"
ARCH=x86_64 "$tool" --appimage-extract-and-run --no-appstream "$appdir" "$image"
echo "$image prêt"
