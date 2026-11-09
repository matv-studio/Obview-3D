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
