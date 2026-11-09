# État d'Obview 3D

Mis à jour : 09/11/2026.

## Fait
- Décisions D-01 à D-18 (`DECISIONS.md`), architecture cible (`ARCHITECTURE.md`).
- `obv-gpu` : appareil Vulkan 1.3, mesh shaders, mémoire, tampons par adresse, pipelines, rendu hors écran.
- `obv-ui` : bibliothèque d'interface maison (D-12) : thèmes clair et sombre, texte, icônes, mise en page,
  survol, clics, info-bulles, animations ; composants (boutons, onglets, recherche, arbre, propriétés, barre
  flottante).
- `obv-io` : lecteur GLB (projection en mémoire, primitives décodées en parallèle). Montre de l'utilisateur :
  246 pièces, 453 278 triangles, lue en 0,9 s (profil de développement).
- `obv-render` : fond en dégradé, géométrie lue par adresse, éclairage de présentation, caméra en orbite.
- `obv-ui` (suite) : entrées clavier et souris indépendantes de la fenêtre (`input.rs`), glisser, double clic,
  focus et saisie (curseur, sélection, clic au caractère), zones défilantes avec barre maison et défilement doux,
  listes virtuelles, calques (menus fermés au clic extérieur sans clic dessous, fenêtres modales avec voile),
  formes du pointeur ; commandes `controls.rs` : interrupteur, case à cocher, liste de choix, curseur, pastilles
  de couleur, champ de saisie, fenêtre modale. Couleur d'accent dérivée par thème avec contraste vérifié (D-14).
  Essais d'interaction sans GPU (`crates/obv-ui/tests/interaction.rs`).
- `apps/obview` : coque (barre du haut, arbre des pièces virtualisé et défilant, recherche réelle, vue 3D, cube
  de vue, barre d'outils flottante, propriétés en unités choisies) ; réglages (`settings.rs`, JSON dans le
  dossier de configuration) : thème sombre / clair / système, accent (8 teintes + valeur libre), 7 schémas de
  navigation avec Blender par défaut (D-13), zoom inversé, zoom vers le pointeur, pivot sur le point visé,
  vitesse du zoom, unités, cube de vue ; fenêtre des réglages (Ctrl+,). Captures 1920 × 1080 (D-16) dans les
  deux thèmes avec références : coque, réglages (navigation, apparence, liste ouverte).
- Fenêtre réelle (D-17) : winit, chaîne d'échange Vulkan, rendu à la demande, glisser-déposer d'un GLB ;
  sélection au pixel par le GPU (clic dans la vue) ; navigation selon le schéma choisi (Blender par défaut),
  orbite autour du point visé, zoom vers le pointeur, vues du pavé numérique, cadrage animé (F) ; raccourcis
  d'outils (V, M, C, E), recherche (Ctrl+F), réglages (Ctrl+,). Vérifiée sous Xvfb avec xdotool.
- Inspecteur des rendus (D-15, D-18) : bandes diagonales Éclairé, Couleur, Normales, Topologie, Rayons X,
  Arêtes (arêtes vives + contours apparents), noms le long des traits, rendu isolé au clic, retour par
  l'étiquette ou Échap, triangles et sommets en bas à gauche (R ou bouton de la barre).
- Étude préalable (« STL Remesh ») et analyse des fichiers d'essai de l'utilisateur : voir D-07.

## Prochaines étapes
1. Rendu : arêtes vives et silhouettes, occlusion ambiante, lumière moins plate ; contour de la sélection.
2. Cube de vue cliquable (faces, arêtes, coins), menu du fichier (ouvrir, récents), dialogue d'ouverture natif,
   chargement en tâche de fond avec progression.
3. Lecteurs STL / OBJ / FBX parallèles.
4. Géométrie en mémoire GPU locale (tampons de transfert), couleurs par pièce sans renvoi de la géométrie.
5. Grappes et niveaux de détail ; tri GPU ; mesh shaders.
6. STEP par OpenCASCADE ; arbre des pièces ; mesure exacte ; coupe ; vue éclatée.
7. Conversion (soudure, simplification avec arêtes vives, écart de Hausdorff, écriture GLB / OBJ / FBX).
