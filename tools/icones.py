#!/usr/bin/env python3
"""Icônes de l'application (logo « V », D-36), tirées du SVG validé (version claire).

Écrit apps/obview/assets/obview.svg, obview-256.png, obview.ico et obview.icns depuis
crates/obv-ui/assets/images/obview-clair.svg (la barre du haut de l'application utilise ce même fichier).
Dépendances : rsvg-convert (librsvg), Pillow.  Usage : python3 tools/icones.py
"""

import io
import os
import shutil
import subprocess

from PIL import Image

ICI = os.path.dirname(os.path.abspath(__file__))
RACINE = os.path.join(ICI, "..")
SOURCE = os.path.join(RACINE, "crates", "obv-ui", "assets", "images", "obview-clair.svg")
SORTIE = os.path.join(RACINE, "apps", "obview", "assets")


def rendu(px: int, marge: int = 0) -> Image.Image:
    """Logo rendu sur `px` - 2 × `marge` pixels, centré dans un carré transparent de `px`."""
    cote = px - 2 * marge
    png = subprocess.run(
        ["rsvg-convert", "-w", str(cote), "-h", str(cote), SOURCE], check=True, capture_output=True
    ).stdout
    logo = Image.open(io.BytesIO(png)).convert("RGBA")
    out = Image.new("RGBA", (px, px), (0, 0, 0, 0))
    out.alpha_composite(logo, (marge, marge))
    return out


def main() -> None:
    shutil.copyfile(SOURCE, os.path.join(SORTIE, "obview.svg"))
    rendu(256).save(os.path.join(SORTIE, "obview-256.png"), optimize=True)
    # Windows : une image par taille (rendue à sa taille, pas réduite).
    tailles = [16, 24, 32, 48, 64, 128, 256]
    images = [rendu(t) for t in tailles]
    images[-1].save(
        os.path.join(SORTIE, "obview.ico"), sizes=[(t, t) for t in tailles], append_images=images[:-1]
    )
    # macOS : gabarit Apple, dessin dans 824 px sur 1024.
    rendu(1024, 100).save(
        os.path.join(SORTIE, "obview.icns"),
        sizes=[(16, 16), (32, 32), (128, 128), (256, 256), (512, 512), (1024, 1024)],
    )


if __name__ == "__main__":
    main()
