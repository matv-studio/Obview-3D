#!/usr/bin/env python3
"""Icônes de l'application (logo « lentille », D-34) : cube isométrique au trait dans un anneau.

Écrit apps/obview/assets/obview.svg, obview-256.png, obview.ico et obview.icns.
Dépendances : rsvg-convert (librsvg), Pillow.  Usage : python3 tools/icones.py
"""

import io
import math
import os
import subprocess
import tempfile

from PIL import Image

ICI = os.path.dirname(os.path.abspath(__file__))
SORTIE = os.path.join(ICI, "..", "apps", "obview", "assets")
FOND = "#1c1c1c"
TRAIT = "#f2f2f2"


def logo(trait: float = 52.0, tuile: float = 1024.0, marge: float = 0.0, petit: bool = False) -> str:
    """Tuile arrondie, anneau, cube au trait (face du dessus teintée). `trait` en unités de 1024.

    `petit` (16 et 24 px) : anneau et cube agrandis pour garder un vide lisible entre eux."""
    k = math.cos(math.pi / 6)
    c = 512.0
    s = 146.0 if petit else 128.0
    rayon = 446.0 if petit else 352.0
    coin = 180.0 if petit else 230.0

    def p(x, y, z):
        return (c + s * (x - z) * k, c + s * ((x + z) * 0.5 - y))

    haut = [p(-1, 1, -1), p(1, 1, -1), p(1, 1, 1), p(-1, 1, 1)]
    hexa = [haut[0], haut[1], p(1, -1, -1), p(1, -1, 1), p(-1, -1, 1), haut[3]]
    chemin = lambda pts: "M" + " L".join(f"{x:.1f} {y:.1f}" for x, y in pts) + " Z"
    centre, bas = haut[2], p(1, -1, 1)
    aretes = (
        f"M{haut[3][0]:.1f} {haut[3][1]:.1f} L{centre[0]:.1f} {centre[1]:.1f} L{haut[1][0]:.1f} {haut[1][1]:.1f} "
        f"M{centre[0]:.1f} {centre[1]:.1f} L{bas[0]:.1f} {bas[1]:.1f}"
    )
    echelle = (tuile - 2 * marge) / 1024.0
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {tuile:g} {tuile:g}">'
        f'<g transform="translate({marge:g} {marge:g}) scale({echelle:g})">'
        f'<rect width="1024" height="1024" rx="{coin:g}" fill="{FOND}"/>'
        f'<circle cx="512" cy="512" r="{rayon:g}" fill="none" stroke="{TRAIT}" stroke-width="{trait:g}"/>'
        f'<path d="{chemin(haut)}" fill="{TRAIT}" fill-opacity="0.22"/>'
        f'<path d="{chemin(hexa)} {aretes}" fill="none" stroke="{TRAIT}" stroke-width="{trait:g}" '
        f'stroke-linecap="round" stroke-linejoin="round"/>'
        "</g></svg>"
    )


def rendu(svg: str, px: int) -> Image.Image:
    with tempfile.NamedTemporaryFile("w", suffix=".svg", delete=False) as f:
        f.write(svg)
    try:
        png = subprocess.run(
            ["rsvg-convert", "-w", str(px), "-h", str(px), f.name], check=True, capture_output=True
        ).stdout
    finally:
        os.unlink(f.name)
    return Image.open(io.BytesIO(png)).convert("RGBA")


def trait_pour(px: int) -> float:
    """Trait épaissi aux petites tailles : au moins ~1,6 pixel réel."""
    return max(52.0, 1.5 * 1024.0 / px)


def main() -> None:
    with open(os.path.join(SORTIE, "obview.svg"), "w") as f:
        f.write(logo() + "\n")
    rendu(logo(), 256).save(os.path.join(SORTIE, "obview-256.png"), optimize=True)

    # Windows : une image par taille, trait adapté.
    tailles = [16, 24, 32, 48, 64, 128, 256]
    images = [rendu(logo(trait_pour(t), petit=t <= 24), t) for t in tailles]
    images[-1].save(
        os.path.join(SORTIE, "obview.ico"), sizes=[(t, t) for t in tailles], append_images=images[:-1]
    )

    # macOS : tuile de 824 dans 1024 (gabarit Apple), tailles de l'icns.
    icns = rendu(logo(52.0, 1024.0, 100.0), 1024)
    icns.save(os.path.join(SORTIE, "obview.icns"), sizes=[(16, 16), (32, 32), (128, 128), (256, 256), (512, 512), (1024, 1024)])


if __name__ == "__main__":
    main()
