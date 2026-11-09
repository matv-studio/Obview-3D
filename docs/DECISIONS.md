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
couleurs en espace linéaire prémultipliées, cible sRGB. Texte : police Inter 4.1 (OFL) en trois graisses, mise en
forme et rendu par swash, position sous-pixel au quart de pixel, couverture ajustée selon la clarté du texte.
Icônes : Lucide (ISC), rendues par resvg au pixel exact de l'écran. Mise en page : arbre reconstruit à chaque
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

