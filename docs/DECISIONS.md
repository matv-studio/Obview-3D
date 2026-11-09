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
