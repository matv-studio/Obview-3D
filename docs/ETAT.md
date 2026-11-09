# État d'Obview 3D

Mis à jour : 09/11/2026.

## Fait
- Décisions D-01 à D-11 (`DECISIONS.md`), architecture cible (`ARCHITECTURE.md`).
- Étude préalable (« STL Remesh ») et analyse des fichiers d'essai de l'utilisateur : voir D-07.

## En cours
- Socle : espace de travail Cargo, couche Vulkan, rendu hors écran, CI.

## Prochaines étapes
1. Socle Vulkan + premier rendu hors écran comparé à une référence.
2. Fenêtre et présentation (Windows, Linux).
3. Interface maison : jetons de thème, texte, premiers widgets, coque de l'application (deux thèmes).
4. Lecteurs STL / OBJ / GLB parallèles ; navigation (orbite, panoramique, zoom vers le curseur, cube de vue).
5. Grappes et niveaux de détail ; tri GPU ; mesh shaders.
6. STEP par OpenCASCADE ; arbre des pièces ; mesure exacte ; coupe ; vue éclatée.
7. Conversion.
