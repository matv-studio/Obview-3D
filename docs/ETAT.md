# État d'Obview 3D

Mis à jour : 10/11/2026.

## Fait
- Décisions D-01 à D-30 (`DECISIONS.md`), architecture cible (`ARCHITECTURE.md`).
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
- Lecteurs STL (binaire, texte), OBJ (parallèle), FBX (ufbx) ; ouverture par dialogue natif (Ctrl+O),
  glisser-déposer ou ligne de commande, lecture dans un fil séparé avec indicateur (D-19).
- Moteur de conversion `obv-mesh` (D-20) : écart garanti mesuré, GLB / OBJ / FBX ; fenêtre « Convertir »
  (format, densité fidélité ↔ performance, écart en mm, silhouette, progression, bilan, ouverture du
  résultat ; D-22) ; en ligne de commande :
  `obview --convert <entrée> <sortie> --tolerance <mm>` (noms français acceptés).
- Gizmo de navigation (D-21) : cube semi-transparent et axes réunis, 26 directions au clic, glisser pour tourner.
- Panneaux latéraux masqués par défaut, ouverts depuis la barre du bas (D-21).
- Passe d'interface sur la charte des apps (D-24) : Geist, gris neutres, accent facultatif et son étendue,
  plateau arrondi, icônes maison 16 × 16, composants refaits ; captures vérifiées en 1920 × 1080 et
  2560 × 1440 dans les deux thèmes.
- Barre du bas en onglet du cadre, boutons choisis dans les réglages ; poignées des panneaux (clic, glisser
  200–520 px) ; gizmo au choix : Axes (défaut), Cube, Trièdre, Aucun (D-25). Vérifié aussi en fenêtre réelle.
- Outils (D-26) : perspective / orthographique, mesure avec accroche, coupe bouchée, vue éclatée, capture PNG
  (2×, 4K, fond transparent), onglet « Vues » (standard et enregistrées par fichier).
- STEP / IGES par OpenCASCADE 8.0.1 statique (D-27) : noms, couleurs par face, instances ; montre réelle lue en
  8,7 s (285 pièces). Flux de version Windows en brouillon (D-28), logo.
- D-29 : schéma souris « Obview » par défaut, outil recliqué → Sélection, Focus (F) sur la pièce
  sélectionnée, double-clic → propriétés, R qui réaffiche les bandes ; transitions (fenêtres, menus, panneaux,
  vues de rendu) ; démarrage sans modèle (zone d'ouverture) ; progression réelle de l'ouverture STEP avec
  annulation, avis d'échec ; GLB quantifié lisible par Blender ; interface en anglais par défaut, français au
  choix. Captures de référence régénérées (anglais) ; nouvelles : vue vide, lecture, avis, français,
  transitions en cours.
- D-30 : compression Draco (bibliothèque de Google, `obv-draco`) en écriture et en lecture GLB ; écart garanti
  tenu avec la quantification (90 % simplification, 10 % quantification, essai de Hausdorff) ; moteur OpenGL
  4.3 de secours (`obv-gl`, mêmes nuanceurs, même image que Vulkan dans les essais), choix et bascule à chaud
  dans Réglages › Affichage ; version macOS Apple Silicon par KosmicKrisp (`tools/macos-sdk.sh`,
  `tools/macos-app.sh`, job CI `macos-26`) ; DirectX 12 écarté. Version 0.2.0.
- Étude préalable (« STL Remesh ») et analyse des fichiers d'essai de l'utilisateur : voir D-07.

## Prochaines étapes
0. Brouillon de version v0.2.0 (Windows x64, macOS arm64) préparé par `version.yml` : à essayer et publier par
   l'utilisateur ; le brouillon v0.1.0 est dépassé. Archives : Windows 10,8 Mo, macOS 14,7 Mo. Application macOS jamais lancée sur un Mac réel :
   en CI (`macos-26`), le chargeur livré trouve le pilote KosmicKrisp, qui ne voit aucun GPU utilisable dans
   la machine virtuelle (pas de Metal 4) ; premier essai à faire par l'utilisateur. Bascule de moteur à chaud vérifiée seulement dans les
   essais. Ensuite : réglage de la finesse du maillage STEP ; arbre d'assemblage (chemins lus, pas encore
   affichés) ; défilement de l'arbre jusqu'à la pièce double-cliquée ; Linux à discuter.
1. Rendu : arêtes vives et silhouettes, occlusion ambiante, lumière moins plate ; contour de la sélection.
2. Fichiers récents ; menu contextuel de la vue (isoler, masquer, cadrer).
3. Géométrie en mémoire GPU locale (tampons de transfert), couleurs par pièce sans renvoi de la géométrie ;
   textures et matériaux par plage dans la vue.
4. Conversion : ajustement final des sommets sur la surface d'origine (formes arrondies).
5. Grappes et niveaux de détail ; tri GPU ; mesh shaders.
6. STEP / IGES par OpenCASCADE 8.0.1 (paquets conda-forge, pont cxx) : arbre d'assemblage, noms, couleurs,
   maillage à flèche et angle maîtrisés ; mesure exacte ; coupe ; vue éclatée.
