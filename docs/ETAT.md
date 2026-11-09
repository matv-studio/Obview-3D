# État d'Obview 3D

Mis à jour : 09/11/2026.

## Fait
- Décisions D-01 à D-12 (`DECISIONS.md`), architecture cible (`ARCHITECTURE.md`).
- `obv-gpu` : appareil Vulkan 1.3, mesh shaders, mémoire, tampons par adresse, pipelines, rendu hors écran.
- `obv-ui` : bibliothèque d'interface maison (D-12) : thèmes clair et sombre, texte, icônes, mise en page,
  survol, clics, info-bulles, animations ; composants (boutons, onglets, recherche, arbre, propriétés, barre
  flottante).
- `obv-io` : lecteur GLB (projection en mémoire, primitives décodées en parallèle). Montre de l'utilisateur :
  246 pièces, 453 278 triangles, lue en 0,9 s (profil de développement).
- `obv-render` : fond en dégradé, géométrie lue par adresse, éclairage de présentation, caméra en orbite.
- `apps/obview` : coque (barre du haut, arbre des pièces, vue 3D, cube de vue, barre d'outils flottante,
  propriétés) rendue hors écran ; essais de capture dans les deux thèmes avec références.
- Étude préalable (« STL Remesh ») et analyse des fichiers d'essai de l'utilisateur : voir D-07.

## Prochaines étapes
1. Rendu : arêtes vives et silhouettes, occlusion ambiante, lumière moins plate ; contour de la sélection.
2. Fenêtre et présentation (winit + swapchain), navigation à la souris (orbite, panoramique, zoom vers le
   curseur), sélection au pixel par la passe de visibilité.
3. Interface : liste défilante virtualisée (arbre), saisie de texte (recherche), menus, cube de vue cliquable.
4. Lecteurs STL / OBJ / GLB parallèles ; navigation (orbite, panoramique, zoom vers le curseur, cube de vue).
5. Grappes et niveaux de détail ; tri GPU ; mesh shaders.
6. STEP par OpenCASCADE ; arbre des pièces ; mesure exacte ; coupe ; vue éclatée.
7. Conversion.
