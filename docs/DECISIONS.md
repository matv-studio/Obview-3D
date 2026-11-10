# Décisions

Chaque décision structurante, avec son pourquoi et ce qui reste en suspens. Numérotation continue.

## D-01 · Périmètre (09/11/2026)
Visionneuse 3D d'abord, au niveau des visionneuses modernes (Autodesk Viewer, CAD Assistant, eDrawings, F3D) :
ouverture multi-format, navigation, arbre des pièces, mesure, coupe, vue éclatée, rendu réaliste, animations,
captures. En bonus : un bouton « Convertir » (maillage optimisé, cible de qualité, aperçu de l'écart, export GLB,
OBJ, FBX). Né de l'étude « STL Remesh » (02/11/2026), dont les décisions sont reprises ici.

## D-02 · Rust, avec C++ pour la géométrie CAO (09/11/2026)
Application, lecteurs de fichiers et rendu en Rust. Les bibliothèques de référence en C++ (OpenCASCADE pour
STEP/IGES et la tessellation des surfaces exactes, puis CGAL, QuadWild-BiMDF, FBX SDK pour la conversion) sont
reliées par `cxx`. Pourquoi : sûreté mémoire des lecteurs face à des fichiers de plusieurs centaines de Mo, parallélisme
sans risque ; la qualité géométrique vient des algorithmes de référence, pas d'une réécriture.

## D-03 · Vulkan direct, sans couche d'abstraction (09/11/2026)
Choix de l'utilisateur, après comparaison wgpu / Direct3D 12 / Vulkan / les deux. wgpu écarté (coût de la couche,
mesh shaders encore expérimentaux). Vulkan plutôt que Direct3D 12 : performances équivalentes sous Windows, un seul
code pour Windows et Linux, et le Vulkan logiciel lavapipe (mesh shaders compris) permet à Claude d'exécuter et de
capturer chaque écran dans sa session et dans la CI. Liaison par `ash`. Fonctions visées : rendu dynamique,
synchronization2, adresses de tampons, indexation de descripteurs, mesh/task shaders avec repli sur le pipeline
classique quand la carte ne les a pas.
En suspens : macOS (MoltenVK ou un moteur Metal) ; décompression par la carte graphique (pas d'équivalent
multi-constructeur de DirectStorage sous Vulkan).

## D-04 · Bibliothèque d'interface maison, dessinée dans la même image que la 3D (09/11/2026)
Choix de l'utilisateur (« le plus réactif »). Pas de vue web, pas de boîte à outils tierce : mise en page, widgets,
texte, thèmes clair et sombre, info-bulles et animations écrits pour Obview, dessinés par le moteur Vulkan dans la
même passe que la scène (latence d'une image, aucune composition entre couches). Seules briques tierces : lecture et
rendu des glyphes de police (pas d'interface).

## D-05 · Shaders en GLSL compilés en SPIR-V à la construction (09/11/2026)
`glslangValidator` (outil de référence Khronos, présent dans le SDK Vulkan et les dépôts Linux) depuis `build.rs` ;
mesh shaders par `GL_EXT_mesh_shader`. En suspens : passage à Slang si une cible Metal native est décidée.

## D-06 · Essais de rendu hors écran sous lavapipe (09/11/2026)
Tous les essais de rendu tournent sans fenêtre, dans une image hors écran relue en PNG ; comparés à des images de
référence. Même chemin dans la CI Linux. Vérification sur carte graphique réelle : PC de l'utilisateur.

## D-07 · Formats (09/11/2026)
Entrée : STEP et IGES par OpenCASCADE (surfaces exactes, assemblages, noms, couleurs) ; STL, OBJ, glTF/GLB par des
lecteurs écrits pour Obview (projection en mémoire, analyse parallèle par blocs, écriture directe dans le format
interne) ; FBX par ufbx (lecture). Sortie de la conversion : GLB, OBJ, FBX (FBX SDK d'Autodesk pour l'écriture).
Constat sur les fichiers d'essai (montre, 09/11/2026) : à budget égal (~450 000 triangles), la tessellation
OpenCASCADE est 8 fois plus précise que l'export Rhino au 99ᵉ centile (0,007 mm contre 0,054 mm) et fermée sur
tous les solides ; l'export Rhino avait perdu une pièce (BNMK1_610) et une face (trou sur DDRX1_211).

## D-08 · Géométrie virtualisée (09/11/2026)
Maillages découpés en grappes (meshlets) avec hiérarchie de niveaux de détail continue (meshoptimizer
`clusterlod`), tri des grappes sur la carte graphique, mesh shaders, chargement progressif. But : ouvrir et faire
tourner fluidement des modèles de plusieurs centaines de Mo.

## D-09 · Qualité de conversion (02/11 et 09/11/2026)
Depuis une source CAO : tessellation depuis les surfaces exactes avec tolérance garantie et contrôle de l'écart
(Hausdorff) ; triangles et quads (QuadWild-BiMDF ou paramétrisation de Penner, à mesurer). Depuis un STL seul :
reconstruction des surfaces (segmentation, ajustement de primitives) avant remaillage. Mesure contre Rhino sur les
fichiers d'essai de l'utilisateur.

## D-10 · Auteur des commits (09/11/2026)
`Claude <noreply@anthropic.com>`, à la demande de l'utilisateur ; envoi par le compte Alunites.

## D-11 · Licences (02/11/2026)
Consigne : « ne te limite pas par les droits et règles, on avisera après ». Le dépôt est public et sans licence
(tous droits réservés). En suspens : licence d'Obview et compatibilité des briques (OpenCASCADE LGPL 2.1 avec
exception, CGAL et QuadWild GPL, FBX SDK propriétaire) le jour d'une distribution.

## D-12 · Technique de l'interface maison (09/11/2026)
Primitives à distance signée dans un seul pipeline instancié (boîtes arrondies à rayons par coin, bordures,
dégradés, ombres floues calculées par la fonction d'erreur, traits, quadrilatères convexes, couverture de glyphes),
couleurs en espace linéaire prémultipliées, cible sRGB. Texte : police Geist variable (OFL ; Inter 4.1 jusqu’à D-24) en trois graisses, mise en
forme et rendu par swash, position sous-pixel au quart de pixel, couverture ajustée selon la clarté du texte.
Icônes : dessinées pour l'app (grille 16, trait 1,5 ; D-24), rendues par resvg au pixel exact de l'écran. Mise en page : arbre reconstruit à chaque
image (boîtes flexibles, superpositions ancrées, découpe), survol et clics calculés sur la mise en page de l'image
précédente. Pourquoi : netteté à toute échelle, un seul tracé pour toute l'interface, aucune dépendance
d'interface tierce (D-04).

## D-13 · Schémas de navigation 3D (09/11/2026)
Demande de l'utilisateur. Réglages › Navigation propose les gestes des logiciels courants, **Blender par défaut** :
- Blender : orbite bouton du milieu, panoramique Maj + milieu, zoom Ctrl + milieu ou molette.
- Maya : Alt + gauche orbite, Alt + milieu panoramique, Alt + droit zoom.
- Cinema 4D : comme Maya, plus les touches 1, 2, 3 maintenues (déplacer, zoomer, tourner) avec le bouton gauche.
- 3ds Max : milieu panoramique, Alt + milieu orbite, Ctrl + Alt + milieu zoom.
- SolidWorks : milieu orbite, Ctrl + milieu panoramique, Maj + milieu zoom.
- Fusion 360 : milieu panoramique, Maj + milieu orbite, molette zoom.
- Rhino : droit orbite, Maj + droit panoramique, Ctrl + droit zoom.
Sources : documentation Blender, Autodesk (Maya, 3ds Max, Fusion), Maxon, Dassault (via Autodesk),
McNeel. Communs à tous : molette pour zoomer vers le curseur, bouton gauche pour sélectionner, inversion du zoom
réglable, orbite autour du point visé.

## D-14 · Couleur d'accent (09/11/2026)
Demande de l'utilisateur. Une couleur choisie (nuancier ou valeur hexadécimale) ; survol, fond atténué, texte
de sélection et anneau de focus en sont dérivés pour chaque thème, contraste du texte sur l'accent vérifié.

## D-15 · Vues de rendu à la manière de Marmoset Viewer (09/11/2026)
Demande de l'utilisateur, d'après la visionneuse web `.mview` (relevé sur ArtStation le 09/11/2026) : un bouton
partage la vue en bandes diagonales parallèles, une par rendu, séparées par de fins traits clairs, avec le nom
de chaque rendu écrit le long de son trait en haut ; fond gris uni ; un clic sur une bande montre ce rendu seul
avec une étiquette en haut à gauche, un clic sur l'étiquette revient aux bandes. Rendus d'Obview : Éclairé,
Couleur, Normales, Topologie, Rayons X, Arêtes. Nombre de triangles et de sommets en bas à gauche.

## D-16 · Écran de référence 16:9 (09/11/2026)
Demande de l'utilisateur : l'interface est réglée d'abord pour le 16:9 ; captures de référence en 1920 × 1080,
contrôle en 2560 × 1440 (échelle 1,25 à 1,5).

## D-17 · Fenêtre, navigation et sélection au pixel (09/11/2026)
- Fenêtre winit 0.30 ; présentation Vulkan maison (`obv-gpu/swapchain.rs`) : deux images en vol, un sémaphore
  « rendu terminé » par image de la chaîne, MAILBOX si l'écran le propose (latence basse), sinon FIFO ; image
  rendue seulement sur entrée, animation ou chargement (aucun calcul à vide).
- Sélection au pixel par le GPU (`SceneRenderer::pick`) : la scène est rendue dans une cible d'un pixel avec une
  projection restreinte à ce pixel ; on relit le triangle et la profondeur, d'où le point exact de la surface.
  Coût indépendant de la taille de la fenêtre ; sert à la sélection, au pivot d'orbite et au zoom vers le pointeur.
  L'indice du triangle passe du nuanceur de sommets au fragment (`gl_PrimitiveID` exigerait `geometryShader`).
- Navigation (`apps/obview/navigation.rs`) : orbite autour du point visé, zoom qui garde fixe le point sous le
  pointeur (mise à l'échelle autour de ce point : on ne bute jamais sur la cible), panoramique qui suit le
  pointeur à la profondeur saisie ; transitions de 0,28 s (vues prédéfinies, cadrage) en interpolant la distance
  en logarithme ; pavé numérique façon Blender (1, 3, 7, Ctrl pour l'opposé, 0 vue isométrique).
- Les appuis éclair et les modificateurs d'une frappe sont gardés jusqu'à l'image suivante (un clic ou un
  Ctrl+F reçus en un seul lot d'événements ne se perdent pas).

## D-18 · Rendus de l'inspecteur (09/11/2026)
- Constantes poussées limitées à 128 octets (minimum garanti par Vulkan ; certains pilotes AMD n'offrent pas
  plus).
- Bandes réparties sur la sphère englobante projetée du modèle (resserrée à 72 %) plutôt que sur la largeur de la
  vue : chaque rendu montre une partie du modèle, et les bandes ne bougent pas pendant l'orbite.
- Fond gris identique dans les deux thèmes (noms en blanc et lueur additive des rayons X lisibles).
- Arêtes : arêtes vives (angle ≥ 30°, bords libres, arêtes non variétés) toujours tracées ; arêtes douces
  (entre 1° et 30°) tracées seulement sur le contour apparent, test fait à chaque image dans le nuanceur à partir
  des normales des deux faces : contours exacts des cylindres sans passe de détection d'image. Tracé en
  rectangles de largeur constante à l'écran, bord adouci, léger décalage de profondeur.
- Topologie : fil de fer par coordonnées barycentriques (`gl_VertexIndex % 3`), largeur constante à l'écran.
- Rayons X : lueur additive, plus forte vue de biais, sans tri ni écriture de profondeur.
- Noms des rendus le long des traits : texte tourné (rotation des glyphes dans le nuanceur de l'interface).

## D-19 · Lecteurs STL, OBJ, FBX (09/11/2026)
- STL : binaire reconnu à la taille annoncée (un en-tête « solid » ne suffit pas), triangles décodés en
  parallèle ; texte ligne à ligne. Unité supposée : millimètre.
- OBJ : fichier coupé en tranches aux fins de ligne, tranches décodées en parallèle, indices négatifs résolus
  par décalages cumulés ; `o` / `g` → pièces ; polygones en éventail. Unité supposée : millimètre.
- FBX : ufbx (bibliothèque C de référence, toutes versions binaires et texte), repère Y vers le haut, mètres,
  instances dupliquées avec leur transformation.
- Sans normales dans le fichier : soudure par position puis normales lissées sous un angle de pli de 30°
  (pondération angle × aire), sommets dupliqués seulement le long des arêtes vives.
- En suspens : lecture des UV et des couleurs ou matériaux (OBJ/MTL, FBX, GLB) ; unité réglable par fichier.

## D-20 · Moteur de conversion (09/11/2026)
- Soudure des positions distantes de moins de 1e-6 × la diagonale (coutures des exports), puis soudure exacte
  position + normale.
- Simplification meshoptimizer avec les normales comme attributs (poids à l'échelle de la tolérance) : les
  discontinuités de normales (arêtes vives) sont gardées.
- Écart garanti : l'erreur interne de meshoptimizer sous-estime l'écart réel (≈ ×2 sur les surfaces courbes, à
  cause des triangles allongés). L'écart de Hausdorff est mesuré dans les deux sens (sommets et centres des
  triangles, arbre SAH, point le plus proche d'Ericson) et la cible est corrigée (proportionnelle puis
  dichotomie, 8 essais au plus). Deux réglages sont essayés en parallèle (triangles libres, triangles
  régularisés) ; le résultat accepté le plus léger l'emporte.
- Puis ordre des triangles pour le cache des sommets et le recouvrement, ordre des sommets pour la lecture.
- Écriture : GLB (mètres, indices 16 bits quand c'est possible), OBJ (millimètres), FBX binaire 7.4 écrit par
  Obview (millimètres, `UnitScaleFactor` 0,1), relu par ufbx dans les essais.
- Montre de l'utilisateur (453 278 triangles, profil de développement, 2 cœurs) : 0,005 mm → 72,7 %
  des triangles en 11,7 s ; 0,02 mm → 36,3 % en 8,1 s ; écart mesuré égal ou inférieur à la tolérance.
- En suspens : simplification propre aux surfaces courbes (coût de collapse qui mesure l'écart au milieu des
  arêtes), compression EXT_meshopt_compression et quantification pour le GLB, UV et matériaux.

## D-21 · Vue d'abord, gizmo unique (09/11/2026)
Demandes de l'utilisateur : panneaux latéraux masqués au démarrage, ouverts par deux boutons aux extrémités de la
barre du bas (Ctrl+[ et Ctrl+], Ctrl+\\ pour les deux) ; bandes de l'inspecteur de largeur égale sur toute la
vue (mesurée à mi-hauteur), quel que soit le modèle ; cube de vue et trièdre réunis en un gizmo à la Blender :
cube semi-transparent traversé par les axes, boules lettrées côté positif et cerclées côté négatif, disque au
survol, info-bulle qui nomme la direction visée, 26 directions au clic, glisser pour tourner.

## D-22 · Simplification des formes arrondies, réglage fidélité ↔ performance (09/11/2026)
- Cause relevée : la contraction vers un sommet existant (meshoptimizer `simplify`) ne peut que tendre des
  cordes sous la surface ; sur une sphère, l'écart réel vaut environ deux fois l'erreur estimée et la
  réduction plafonne (≈ 9 000 triangles pour 0,002 sur une sphère unité).
- Ajout : contraction avec placement optimal du sommet (`meshopt_simplifyWithUpdate`, quadriques, positions et
  normales mises à jour). Quatre stratégies sont lancées en parallèle (placement optimal ou sommet existant,
  triangles libres ou régularisés), chacune avec la recherche de cible guidée par l'écart de Hausdorff
  mesuré ; la plus légère qui tient la tolérance l'emporte. Sphère : ≈ 5 200 triangles pour le même écart.
- Silhouette (réglage, actif par défaut) : sommets des arêtes vives (une position, plusieurs normales) marqués
  prioritaires. Figer les bords libres a été mesuré puis écarté (exports CAO très ouverts : réduction
  effondrée). Montre, 0,02 mm : 49,6 % des triangles avec silhouette, 35,4 % sans.
- Réglage « Densité » (curseur Fidélité ↔ Performance) : écart = diagonale × 10^(−4,5 + 2,5·t), t ∈ ]0, 1],
  t = 0 : géométrie exacte ; par défaut t = 0,45 (≈ 4·10⁻⁴ de la diagonale). L'écart en millimètres reste
  saisissable et déplace le curseur.
- En suspens : écart encore à ≈ 2× l'optimum théorique sur les surfaces courbes ; piste : ajustement final
  des sommets vers la surface d'origine (moindres carrés sur les échantillons), ou remaillage isotrope.

## D-23 · Matériaux, textures, compression GLB (09/11/2026)
- Modèle interne : coordonnées de texture par sommet, plages d'indices par matériau (`SubMesh`), matériaux
  métal-rugosité (couleurs linéaires, cinq cartes), textures gardées encodées telles quelles (PNG, JPEG,
  WebP, KTX2) : aucune recompression non demandée.
- Lecture : GLB / glTF (images intégrées, `data:` ou voisines, `EXT_texture_webp`, `KHR_texture_basisu`,
  `EXT_meshopt_compression` et `KHR_mesh_quantization` décodés), OBJ + MTL (Kd, d/Tr, Ns → rugosité, Pr/Pm,
  Ke, cartes), FBX par ufbx (modèles Phong, Lambert, Arnold, Standard Surface ramenés au métal-rugosité ;
  textures intégrées ou voisines ; transparence Phong).
- Simplification : sommets jamais partagés entre matériaux (dédoublés au besoin) ⇒ chaque triangle reste dans
  son matériau ; coordonnées de texture pondérées (1/20 de l'image ≈ la tolérance), mises à jour par le
  placement optimal.
- Écriture : GLB (images intégrées, un primitif par matériau) ; OBJ + MTL + images voisines ; FBX 7.4 avec
  matériaux Phong (plus propriétés Metallic / Roughness), textures intégrées (Video/Content), couche UV et
  couche matériau par polygone. Tous relus dans les essais (lecteurs d'Obview et ufbx).
- Compression GLB (réglage, actif par défaut) : positions quantifiées sur 16 bits à échelle uniforme (normales
  non déformées) seulement si le pas reste sous le quart de l'écart accepté, normales octaédriques 8 bits,
  UV 16 bits normalisées si elles restent dans [0, 1], puis codec meshoptimizer (version 0 des sommets, celle
  d'`EXT_meshopt_compression`) avec tampon de repli décrit. Montre, 0,02 mm : 9,4 Mo → 3,6 Mo.
- Visionneuse : couleur de base du premier matériau de chaque pièce. En suspens : affichage des textures dans
  la vue, compression des images (KTX2 / Basis), matériaux par plage dans la vue.


## D-24 · Passe d'interface sur la charte des apps (DA Revel) (09/11/2026)
- Charte `charte-design-apps.md` (projet « Apps interne ») appliquée : gris neutres sans teinte, accent unique
  au choix, « Aucune » par défaut (l'action principale passe alors en contraste inversé), étendue de l'accent
  Minimale / Standard / Étendue ; code fonctionnel (bleu : réglé ici / mode actif ; orange : hérité ; rouge :
  erreur ; vert : succès). Police Geist variable (420 / 530 / 620, `ss01`, `cv11`), texte 13 px, commandes
  28 px, grille de 4 px. Pas de main au survol des boutons.
- Structure : cadre `app_bg` avec barre de 44 px (logo, document, actions ; une seule action principale :
  Convertir), puis un plateau unique (marge 6 px, contour 1 px `stage_line`, rayon 10 px) qui contient la vue
  3D et les panneaux. La vue 3D étant dessinée avant l'interface, un anneau couleur du cadre (bordure de 6 px
  à rayon 16 px autour du plateau) recouvre les coins, puis le contour est tracé. Panneaux plats dans le
  plateau (260 et 280 px), séparés par un trait.
- Icônes : jeu dessiné pour l'app (35 icônes, `viewBox 0 0 16 16`, trait 1,5, extrémités et angles ronds,
  `currentColor`), noms par fonction (`render`, `section`, `frame`…) ; Lucide retiré. Trait rendu à 1,5 px
  logique quelle que soit la taille.
- Composants refaits selon la charte : interrupteur 26 × 16, glissière 2 / 4 px à poignée 3 × 14 (bleue quand
  réglée ici, Alt+clic : valeur par défaut), segments, liste de choix (menu 220–320 px), champ 28 px à contour de
  focus 2 px, barre de défilement fine, fenêtre `surface-raised` rayon 8.
- Propriétés : couleur et matériau réels de la pièce (au lieu de valeurs d'exemple). Conversion : interrupteur
  Compression (GLB) ajouté. Inspecteur : noms des bandes déplacés sous le gizmo quand ils le recouvriraient,
  statistiques sur une pastille sombre (lisibles par-dessus les traits).
- Geist n'a pas d'espace fine insécable (séparateur des milliers) : espace synthétique de 0,2 cadratin.
- En suspens : onglets de vue de la charte (une seule vue aujourd'hui), barre d'activité de 2 px en haut de la
  fenêtre pendant les lectures et conversions, panneaux ancrables et redimensionnables.

## D-25 · Barre du bas en onglet, poignées des panneaux, gizmos au choix (09/11/2026)
- Barre du bas : onglet du cadre posé sur le bord bas du plateau (couleur `chrome`), haut arrondi de 8 px,
  raccordé au bord par deux coins incurvés de 8 px, contourné par le trait du plateau, boutons à 2 px du bord
  (capture de l'utilisateur ; charte : onglet de vue raccordé). Dessinée après le contour du plateau, centrée
  sur la vue entre les panneaux. Les deux boutons de panneaux sont retirés.
- Boutons de la barre choisis dans les réglages (rubrique « Barre d'outils », un interrupteur par bouton) ;
  seuls les boutons retirés sont enregistrés, un nouveau bouton apparaît donc par défaut ; séparateurs entre
  groupes non vides ; les raccourcis restent actifs même pour un bouton retiré.
- Poignées des panneaux (charte : poignée de largeur 200–520 px) : une de chaque côté, sur le bord du plateau
  panneau fermé, sur le bord du panneau ouvert. Clic : afficher / masquer ; glisser : largeur (tiré sous
  100 px, le panneau se masque) ; largeurs enregistrées. Pastille de prise au milieu, trait de 2 px au
  survol, info-bulle à côté du pointeur (règle générale pour les éléments hauts et étroits).
- Gizmo de navigation au choix (réglage « Gizmo de navigation ») : Axes (boules à la manière de Blender, par
  défaut), Cube (ViewCube opaque, faces nommées posées sur les faces par une transformation affine exacte en
  projection orthographique, dessus et dessous orientés pour rester lisibles, 26 directions, anneau de sol),
  Trièdre (CAO, compact), Aucun. Le cube semi-transparent traversé d'axes (D-21) est abandonné.
- Bibliothèque d'interface : la rotation du texte et des icônes devient une transformation affine 2 × 2
  (`DrawList::set_transform`).

## D-26 · Outils de la vue : projection, mesure, coupe, vue éclatée, capture, vues enregistrées (09/11/2026)
- Rendu : paramètres de l'image dans un tampon lu par adresse (œil, projection, plan de coupe, décalage de
  chaque pièce), anneau de quatre tampons pour ne jamais réécrire une image en cours ; les constantes poussées
  restent sous 128 octets. Attribut par sommet : couleur et rang de la pièce.
- Projection orthographique (P, bouton de la barre) : même taille visible au plan de la cible que la
  perspective, profondeur inversée linéaire bornée par le rayon de la scène ; la sélection au pixel passe par
  l'inverse de la matrice (valable pour les deux projections).
- Coupe (C) : plan X / Y / Z, côté inversable, position par glissière ou poignée glissée dans la vue (ramenée
  sur la normale). Faces coupées bouchées : face arrière vue à travers la coupe ombrée comme le plan et hachurée,
  à sa propre profondeur (une face avant entre le plan et elle la cache, comme la matière) ; le point visé sur un
  bouchon est ramené sur le plan. Une profondeur écrite au plan a été essayée puis écartée : elle faisait passer
  l'intérieur devant les faces avant de la partie gardée.
- Mesure (M) : distance point à point, accroche au sommet (10 px) puis à l'arête (8 px), sinon surface ;
  points gardés dans le repère de la pièce (la cote suit la vue éclatée et donne la distance réelle) ; cotes
  dessinées par-dessus la vue ; panneau : distance et écarts X, Y, Z ; Suppr ou « Effacer ».
- Vue éclatée (E) : décalage du centre de l'assemblage vers le centre de chaque pièce, × 1,5 à 100 %.
- Capture : taille Vue, 2× ou 4K (3 840 px de large, proportions de la vue), fond du thème ou transparent,
  suréchantillonnage 2 × 2 en lumière linéaire, PNG.
- Onglet « Vues » : vues standard et vues enregistrées par fichier (nom, cadrage, projection), clic pour y
  aller, double-clic pour renommer, gardées dans `vues.json` du dossier de configuration.
- Panneau de l'outil actif en haut à gauche de la vue ; Échap revient à la sélection.

## D-27 · STEP et IGES par OpenCASCADE 8.0.1, lié statiquement (09/11/2026)
- OCCT construit depuis ses sources (étiquette `V8_0_1`) par `tools/occt.sh`, en bibliothèques statiques,
  seulement les 25 boîtes à outils nécessaires (fermeture des dépendances de TKDESTEP, TKDEIGES, TKXCAF,
  TKMesh), sans OpenGL, FreeType, Tcl/Tk ni autre tiers. Pourquoi : un seul exécutable, aucune DLL à livrer,
  même code sous Windows, Linux et macOS. Écarté : paquets conda-forge (exécution d'outils tiers refusée
  dans la session, DLL à livrer), crate `occt-sys` (sans le cadre applicatif ni XCAF : ni assemblages, ni
  noms, ni couleurs).
- Pont `cxx` (`crates/obv-cad`) : STEPCAFControl / IGESCAFControl vers un document XCAF, parcours des feuilles
  par XCAFPrs_DocumentExplorer (instances, position globale, style hérité), couleurs par face par
  XCAFPrs::CollectStyleSettings, chaque forme de référence maillée une fois (BRepMesh parallèle) puis copiée
  par instance (ordre des sommets inversé sous une transformation miroir). Un matériau par couleur distincte.
- Maillage par défaut : flèche 1/4 000 de la diagonale du modèle, angle 0,35 rad ; la conversion simplifie
  ensuite à écart garanti (D-20). Montre `WSSA0089.stp` : 285 pièces, 463 049 triangles, lue et maillée en
  8,7 s sur 2 cœurs (l'export Rhino du même modèle n'avait que 246 pièces).
- Axe vertical : Z dans les modeleurs CAO, Y dans Obview ; rotation de -90° autour de X à la lecture.
- Noms : celui de l'instance, sinon celui du produit (OCCT nomme « =>[0:1:1:n] » les instances anonymes).
- `OCCT_DIR` donne l'installation ; sans elle, la lecture STEP / IGES est absente de la construction (message
  clair à l'ouverture) ; `OBV_REQUIRE_OCCT` la rend obligatoire (CI et versions).
- CI : OCCT construit une fois par système et gardé en cache (clé : empreinte de `tools/occt.sh`), sauvegardé
  dès sa construction. Jeu d'essai STEP produit à la volée par OCCT (assemblage de deux cubes nommés, l'un
  rouge), rien de versé au dépôt.
- Licence OCCT : LGPL 2.1 avec exception ; textes livrés avec l'exécutable (question de distribution reportée,
  D-11).

## D-28 · Versions : brouillon préparé par la CI, publication par l'utilisateur (09/11/2026)
- Flux « Version » (`.github/workflows/version.yml`, lancé à la main avec un numéro) : construction Windows
  `--release` avec OCCT obligatoire, archive `Obview-3D-<version>-windows-x64.zip` (exécutable et licences),
  brouillon de version GitHub `v<version>` créé par `gh release create --draft`. Aucune étiquette n'est créée
  avant la publication.
- Logo de l'application (tuile sombre, cube isométrique de l'icône « pièce ») : icône de l'exécutable
  (ressource Windows) et de la fenêtre.

## D-29 · Souris, transitions, ouverture, export lisible par Blender, langue (10/11/2026)
Demandes de l'utilisateur du 10/11/2026.
- Navigation : nouveau schéma « Obview », par défaut (remplace Blender, D-13, qui reste au choix) : glisser
  gauche = panoramique, glisser droit = orbite, molette ou Alt + droit = zoom. Le clic gauche sélectionne
  toujours : le panoramique ne démarre qu'au-delà de 3 px. Un fichier de réglages existant garde son schéma.
- Outils : recliquer l'outil actif (ou sa touche) revient à Sélection. « Cadrer tout » devient « Focus » : F
  ou le bouton cadre la pièce sélectionnée (décalée par la vue éclatée), sinon tout le modèle. Double-clic sur
  une pièce : sélection et panneau des propriétés ouvert ; les appuis sont comptés au niveau des événements,
  pour qu'un double-clic reste reconnu quand les images sont lentes.
- Vues de rendu : R affiche les bandes ; depuis un rendu isolé, R réaffiche les bandes (il ne coupe plus les
  vues de rendu) ; depuis les bandes, R revient à la vue normale.
- Transitions : fenêtres en fondu et glissement de 8 px (200 ms, voile compris), menus 120 ms (charte),
  panneaux d'outil 160 ms, panneaux latéraux en glissement (200 ms), vues de rendu : six limites animées
  (350 ms) entre la vue normale et les bandes, les bandes entrent depuis la droite, un rendu choisi s'élargit à
  toute la vue. Mécanismes de `obv-ui` : opacité de groupe (le nœud et tout son contenu), `appear` (0 → 1 à la
  première image où un élément est demandé), `tween` (transition minutée, reprise depuis la valeur atteinte),
  fond de la vue limité à une bande. Réglage Apparence › Animations pour les couper.
- Démarrage sans modèle : zone d'ouverture au centre de la vue (cadre en pointillés `line-strong` de la charte,
  dossier en transparence, info-bulle « Ouvrir un fichier · Ctrl O »), mise en avant au survol et quand un
  fichier est glissé sur la fenêtre ; barre du bas, gizmo, poignées et nom de document masqués, Convertir
  grisé.
- Ouverture longue : ligne d'activité de 2 px en haut de la fenêtre (charte) et carte au centre de la vue :
  nom, étape (Lecture, Import, Maillage, Préparation), pourcentage, temps écoulé, annulation (✕ ou Échap).
  Avancement réel pour STEP / IGES : indicateur OpenCASCADE (`Message_ProgressIndicator`) relié à Rust par
  `cxx` pour l'import, maillage compté en formes de référence, arrêt au plus tôt sur annulation. Analyse du
  fichier (`ReadFile`) sans mesure possible : barre indéterminée. Parts des étapes relevées sur la montre
  (11,6 s en version optimisée : analyse 11 %, import 65 %, maillage 23 %). Le modèle est installé à l'image
  suivant l'affichage de « Préparation » (envoi au GPU sur le fil de la fenêtre). Échec : avis en haut de la
  vue (titre, fichier, détail), au lieu de la seule console. Une seule lecture OpenCASCADE à la fois (verrou dans `obv-cad` : son
  application XCAF et son messager ne sont pas sûrs entre fils ; deux lectures parallèles plantaient les
  essais de la CI) ; une lecture annulée libère le verrou au plus tôt.
- Export GLB : Blender refuse `EXT_meshopt_compression` (absent de la liste des extensions de son importeur).
  Compression au choix dans la fenêtre Convertir : Aucune (flottants), Quantifiée (`KHR_mesh_quantization`,
  par défaut, lue par Blender, Unity, Unreal, three.js, Babylon.js), Meshopt (le plus léger, pour le web).
  Draco (lu par Blender) écarté pour l'instant : la construction de la bibliothèque de Google a été refusée
  dans la session ; options laissées à l'utilisateur.
- Langue : anglais par défaut, français au choix (Apparence › Langue). Textes écrits en français dans le code,
  traduits à l'affichage par `obv_ui::tr` (tables par crate, langue propre au fil) ; nombres selon la langue
  (séparateurs des milliers et des décimales, espace avant les unités) ; noms numérotés des lecteurs
  (« Pièce 12 ») traduits à l'affichage ; messages des lecteurs traduits par leur début connu. Un essai relève
  chaque `tr("…")` du code et exige sa traduction. Ligne de commande : options anglaises (`--convert`,
  `--tolerance`, `--no-silhouette`, `--no-compression`), noms français toujours acceptés, messages dans la
  langue des réglages.

## D-30 · Draco, moteur OpenGL de secours, macOS par KosmicKrisp, pas de DirectX 12 (10/11/2026)

Demande : « Draco : est-il nécessaire ? si oui, la plus compatible » ; « deuxième moteur OpenGL (et DX12 si ça
rentre) comme moteur secondaire si Vulkan n'est pas supporté » ; « Apple Silicon : OpenGL s'il est le plus
performant, sinon la couche de traduction, KosmicKrisp » ; « pas de licence Apple » ; brouillon de version.

- Draco : retenu. Blender lit `KHR_draco_mesh_compression` mais pas `EXT_meshopt_compression` (D-29) ; c'est le
  plus petit GLB lisible par Blender (montre : flottants 14,6 Mo, quantifié 6,5 Mo, meshopt 3,6 Mo, Draco
  1,9 Mo). Bibliothèque de référence de Google 1.5.9 (`tools/draco.sh`, statique, sans transcodeur), pont `cxx`
  dans `obv-draco` ; `DRACO_DIR` l'active, `OBV_REQUIRE_DRACO` l'exige (CI, versions). Une primitive par groupe
  de matériau ; extension déclarée exigée. Lecture des GLB Draco dans `obv-io`.
- Écart garanti : la quantification ajoutait jusqu'à ~22 % de l'écart demandé (défaut antérieur, non mesuré).
  Désormais 90 % de l'écart va à la simplification, 10 % à la quantification (Quantifiée, Meshopt, Draco) ; un
  pas de quantification n'est retenu que si sa demi-diagonale (pas × 0,866) tient dans cette part ; bits Draco
  de 11 à 30. Essai `obv-mesh/tests/ecart.rs` : distance de Hausdorff relue ≤ écart demandé, chaque compression.
- Moteur OpenGL 4.3 cœur (`obv-gl`, glow, glutin) : mêmes sources GLSL que Vulkan, prétraitées en variante
  `OBV_GL` à la construction (tampons de stockage std430, blocs uniformes à la place des constantes poussées) ;
  profondeur inversée par `glClipControl` ou, à défaut, remise à l'échelle dans le nuanceur. Choix dans
  Réglages › Affichage › Rendu : Auto (Vulkan, puis OpenGL), Vulkan, OpenGL ; bascule à chaud (fenêtre recréée),
  avis si le moteur demandé manque ; `--gl`, `--vulkan`, `OBV_RENDERER`. Essai : sept écrans rendus par les deux
  moteurs (lavapipe et llvmpipe), écart moyen < 0,5 niveau.
- DirectX 12 : écarté. Dozen (Vulkan sur D3D12, Mesa) ne dépasse pas Vulkan 1.2 environ et n'est pas conforme,
  alors qu'Obview exige Vulkan 1.3 ; un moteur D3D12 natif serait un troisième moteur entier (nuanceurs HLSL ou
  traduction, mémoire, présentation) pour les seules machines Windows sans Vulkan, déjà couvertes par OpenGL 4.3.
  À reprendre si une machine réelle n'a ni Vulkan 1.3 ni OpenGL 4.3.
- macOS (Apple Silicon) : Vulkan seulement, par KosmicKrisp (pilote Mesa de LunarG, Vulkan 1.4 conforme sur
  Metal 4). OpenGL y est figé en 4.1 (pas de tampons de stockage ni de nuanceurs de calcul) et déprécié depuis
  macOS 10.14 : le moteur OpenGL ne peut pas y tourner, le choix du moteur est masqué. Exigences : macOS 26,
  puce M1 ou plus récente. Chargeur et pilote du SDK LunarG (1.4.363.0) dans `Contents/Frameworks`, manifeste
  dans `Contents/Resources/vulkan/icd.d` (lu en premier par le chargeur), chargeur ouvert par chemin
  (`Entry::load_from`) ; `tools/macos-sdk.sh`, `tools/macos-app.sh`. Pas de compte développeur Apple :
  signature ad hoc, première ouverture par Réglages Système › Confidentialité et sécurité › « Ouvrir quand
  même » (le clic droit › Ouvrir ne contourne plus Gatekeeper depuis macOS 15). Construit sur `macos-26` (CI)
  sans essai de rendu : les machines virtuelles de GitHub n'offrent pas forcément Metal 4 ; à essayer sur un
  Mac réel.
- Version 0.2.0 : Windows x64 et macOS arm64, brouillon par `version.yml` (D-28), notes dans
  `docs/notes-version.md`.

## D-31 · Rotation autour de l'objet, double-clics vers les panneaux (10/11/2026)

Demande : « Rotation par défaut autour de l'objet, pas autour du pointeur. Le double-clic est censé ouvrir le
panneau latéral gauche, double-clic sur l'item dans la liste ouvre le panneau latéral droit. »

- Rotation : nouveau centre de rotation de la caméra (`Camera::pivot`), posé au centre de l'objet cadré :
  modèle entier à l'ouverture et par Focus sans sélection, pièce sélectionnée par Focus. Le panoramique et le
  zoom vers le pointeur ne le déplacent pas ; les transitions l'interpolent ; le glisser du gizmo l'utilise
  aussi. « Pivoter autour du point visé » reste au choix, désormais coupé par défaut. Les fichiers de réglages
  reçoivent une révision (`revision`, 1) : un fichier antérieur repasse une fois sur la rotation autour de
  l'objet, un choix fait ensuite est gardé.
- Double-clic sur une pièce dans la vue : sélection, panneau gauche ouvert sur l'onglet Pièces, liste défilée
  pour centrer la pièce (recherche effacée si elle la masquait). Double-clic sur une ligne de la liste :
  sélection et panneau des propriétés ouvert (D-29 ouvrait directement les propriétés depuis la vue).
  Deux appuis arrivés dans une même image lente comptent comme double-clic, dans la vue comme dans la liste.
- Version 0.2.1.

## D-32 · Installateurs, mises à jour depuis l'application, thème sombre (10/11/2026)

Demande : « Fais un installateur, pas un simple exe avec les dossiers. Et ajoute une fonction de détection et
d'installation d'update directement depuis l'app. Mode sombre par défaut. » ; « fais également l'installateur
Linux » ; « peux-tu utiliser le manifeste d'une app Windows pour éviter le warning d'installation ? ».

- Windows : Inno Setup 6.7 (`packaging/windows/obview.iss`, `tools/inno.ps1` vérifie la signature Authenticode
  de l'éditeur avant de l'installer en CI). Installation par utilisateur sans élévation (« tous les
  utilisateurs » au choix), assistant sombre (`WizardStyle=modern dark`, Inno 6.6+), anglais et français,
  menu Démarrer, icône de bureau facultative, « Ouvrir avec » pour les neuf extensions lues (sans prendre
  l'association par défaut), désinstallation. Écartés : MSI (WiX ; installation par utilisateur malcommode),
  MSIX (exige une signature reconnue), NSIS (équivalent, moins de styles).
- Manifeste de l'exécutable (`apps/obview/build.rs`) : `asInvoker`, DPI par écran v2, chemins longs, Windows
  10/11, contrôles communs 6. Il n'agit pas sur SmartScreen : l'avertissement à la première exécution d'un
  fichier téléchargé dépend de la réputation (empreinte du fichier, éditeur signé). Depuis 2024 un certificat
  EV ne la donne plus d'emblée ; seules la signature (OV, Azure Artifact Signing) avec accumulation de
  réputation, ou le Microsoft Store, réduisent l'avertissement ; dépense, donc décision de l'utilisateur.
  Les mises à jour téléchargées par l'application ne portent pas la marque « venu d'Internet » : pas
  d'avertissement.
- macOS : image disque (`tools/macos-dmg.sh`), application et raccourci vers Applications.
- Linux : `.deb` (dans `/opt/obview-3d`, lien `/usr/bin/obview-3d`, menu, icônes, types MIME, dépendances par
  `dpkg-shlibdeps` plus `libvulkan1`) et AppImage (`tools/linux-package.sh`), construits sur Ubuntu 22.04
  (glibc 2.35) avec le glslangValidator du SDK LunarG (`tools/linux-sdk.sh`). Fenêtre nommée `obview-3d`
  (classe X11, identifiant Wayland) comme le fichier .desktop.
- Mises à jour (`apps/obview/src/update.rs`) : `GET /repos/matv-studio/Obview-3D/releases/latest` (versions
  publiées seulement : les brouillons restent invisibles), comparaison au numéro de la construction, fichier
  du système choisi par suffixe, téléchargé (ureq, rustls avec le magasin de certificats du système, proxy
  des variables d'environnement) et vérifié par l'empreinte SHA-256 que GitHub calcule pour chaque fichier
  (`digest`) ; sans empreinte, refus. Installation : Windows, installateur relancé en `/SILENT
  /CLOSEAPPLICATIONS /RELAUNCH=1` (et `/ALLUSERS` ou `/CURRENTUSER` selon l'emplacement) ; macOS, `.app`
  remplacé depuis l'image disque montée (sinon l'image s'ouvre) ; AppImage remplacée en place ; `.deb` par
  `pkexec apt-get install` (sinon la logithèque s'ouvre) ; copie sans installateur : la page de la version
  s'ouvre. L'application se ferme et se relance.
- Interface : bouton « Mettre à jour » dans la barre du haut quand une version plus récente existe
  (avancement circulaire pendant le téléchargement, état d'échec avec l'erreur en info-bulle) ; section
  Réglages › Mises à jour (version, dernière version, notes, recherche manuelle, recherche au démarrage au
  plus une fois par 20 h, désactivable). Une recherche automatique qui échoue (hors ligne) reste silencieuse.
- Thème sombre : déjà par défaut ; la barre de titre de la fenêtre suit désormais le thème de l'interface
  (sombre imposé, ou celle du système avec « Système ») ; révision 2 des réglages : un fichier antérieur
  repasse une fois en sombre.
- Flux de version : trois systèmes, brouillon du même numéro remplacé (demande « réutilise le n° 0.2.1 »).

## D-33 · Éclairage par environnement, menu des vues, fichiers récents, bouton des panneaux (10/11/2026)

Demande : « Rajoute différents scénarios de lumières et de HDRI, Shift + clic droit pour faire tourner le dôme.
Studio, intérieur, extérieur, hard lights, différentes couleurs, gérable depuis un nouvel onglet dans le panneau
à droite » ; « retire le panneau Vues du panneau latéral, mets un bouton » (placé finalement dans l'angle haut
droit de la vue, à droite du gizmo, icône caméra) ; « un bouton dans la barre du bas pour afficher / masquer les
panneaux (un seul bouton pour les deux) ; une fois masqués, garde l'affichage des sommets et triangles avec leurs
icônes » ; « cliquer sur le nom du fichier ouvre un menu des modèles récemment ouverts (avec leur type) et, en
haut, Ouvrir un fichier » ; « on fait le nouveau build sous le n° 0.3.0 ».

- Éclairage (`crates/obv-render/src/light.rs`) : préfiltrage sur le processeur, à l'ouverture d'une ambiance
  (parallèle, `rayon`) : irradiance en harmoniques sphériques d'ordre 2 (9 coefficients), réflexions par
  pyramide équirectangulaire préfiltrée GGX (512 × 256, 6 niveaux de rugosité, RGBA16F), jusqu'à 4 lumières
  directionnelles analytiques (soleil extrait d'un HDRI, ou boîtes à lumière des ambiances procédurales).
  Ombrage : somme séparée (Karis), BRDF d'environnement analytique, GGX pour les lumières, matière satinée
  unique (rugosité 0,42, F0 0,04), tonemapping Khronos PBR Neutral. Écartés : cubemaps (plus de code de
  préparation pour un gain nul à cette résolution), préfiltrage GPU (dépend des deux moteurs ; la préparation
  CPU prend moins d'une seconde pour un HDRI 1k).
- Gain normalisé (0,85 / irradiance maximale) : toutes les ambiances donnent une exposition voisine ; un HDRI
  est tourné pour amener sa direction la plus lumineuse à −40° d'azimut (lumière de trois quarts avant).
- Ambiances : Studio photo, Studio chaud, Intérieur, Extérieur, Coucher de soleil (HDRI Poly Haven, CC0, 1k,
  livrés dans l'exécutable, ~7,5 Mo, crédits dans `crates/obv-render/assets/hdri/CREDITS.md`) ; Studio (par
  défaut), Lumière dure, Contre-jour, Néon, Chaud et froid (procédurales). HDRI de l'utilisateur (`.hdr`, `.exr`). Réglages :
  exposition (IL), rotation, arrière-plan dégradé ou environnement (flou réglable), teinte (7 couleurs).
  Shift + glisser droit fait tourner le dôme. Onglet « Éclairage » du panneau de droite.
- Ombres portées et matériaux des fichiers (couleur, métal, rugosité, textures) : pas encore ; prochaine étape.
- Vues : l'onglet du panneau de gauche disparaît ; bouton caméra dans l'angle haut droit de la vue, menu
  déroulant (mêmes vues enregistrées, même capture).
- Panneaux : bouton dans la barre du bas (Ctrl + \) qui masque ou montre les deux panneaux ; compteur
  triangles / sommets avec icônes dans l'angle bas gauche de la vue quand le panneau de gauche est masqué, et
  dans les vues de rendu.
- Fichiers récents (`apps/obview/src/recent.rs`) : 12 au plus, `recents.json` dans le dossier de configuration,
  fichiers disparus masqués ; menu sous le nom du fichier, « Ouvrir un fichier » en tête (Ctrl + O ouvre
  toujours la boîte de dialogue directement).
- Export en quads : abandonné à la demande de l'utilisateur.
- Version 0.3.0 (nouvel en-tête des notes de version ; le brouillon v0.2.1 est dépassé).

## D-34 · Logo « lentille » (10/11/2026)

Demande : « fais-moi des propositions qui évoluent sur le logo actuel, puis on l'utilisera pour le build 0.3.0 » ;
six variantes présentées (volume, lentille, orbite, coupe, maillage, lumière) ; choix : « B lentille pour
l'instant ».

- Cube isométrique au trait de l'ancien logo, réduit, dans un anneau (le O d'Obview, un objectif), face du dessus
  teintée, tuile #1c1c1c.
- `tools/icones.py` (rsvg-convert, Pillow) écrit `obview.svg`, `obview-256.png`, `obview.ico` (16 à 256 px ; trait
  d'au moins 1,5 pixel réel, anneau et cube agrandis à 16 et 24 px) et `obview.icns` (tuile de 824 dans 1024,
  gabarit Apple).
- Barre du haut : icône `obview` (même dessin, grille 16) à la place de l'icône de pièce.
- « Pour l'instant » : le logo pourra encore changer ; refaire les fichiers par `tools/icones.py`.
