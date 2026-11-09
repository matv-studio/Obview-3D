# Architecture d'Obview 3D

Cible, pas état : ce qui est fait est suivi dans `ETAT.md`.

## Vue d'ensemble

```
apps/obview ── fenêtre, boucle d'évènements, commandes, réglages
   ├─ obv-ui      interface maison : mise en page, widgets, texte, thèmes, info-bulles, animations
   ├─ obv-render  rendu de la scène : grappes, niveaux de détail, tri GPU, ombrage, arêtes, coupe, sélection
   │    └─ obv-gpu  Vulkan (ash) : appareil, mémoire, transferts, pipelines, cibles, présentation
   ├─ obv-scene   modèle en mémoire : arbre d'assemblage, pièces, matériaux, unités, métadonnées
   ├─ obv-io      lecteurs STL/OBJ/glTF (maison, parallèles), FBX (ufbx), écrivains GLB/OBJ/FBX
   ├─ obv-cad     pont C++ vers OpenCASCADE (STEP/IGES, surfaces exactes, tessellation, mesure exacte)
   └─ obv-mesh    traitement géométrique : soudure, grappes et niveaux de détail, conversion, écart
```

## Rendu (obv-gpu, obv-render)

- Vulkan 1.3 minimum : rendu dynamique, synchronization2, adresses de tampons, indexation de descripteurs (tous
  les tampons et textures accessibles par indice), sémaphores à échéancier.
- Une image = une liste de commandes : passe de visibilité (identifiant de pièce et de grappe par pixel), passe
  d'ombrage, arêtes et silhouettes en post-traitement à partir des profondeurs et normales, interface par-dessus.
  La passe de visibilité sert aussi à la sélection au pixel près sans lancer de rayons sur le processeur.
- Géométrie : grappes de 64 sommets / 124 triangles ; hiérarchie de niveaux de détail ; tri des grappes sur la
  carte (champ de vision, profondeur de l'image précédente, erreur projetée < 1 pixel) ; mesh shaders quand
  disponibles, sinon tampons d'indices compactés et tracés indirects.
- Transferts : file de copie dédiée quand elle existe, tampons de passage circulaires, chargement progressif (le
  modèle apparaît grossier puis s'affine).
- Hors écran : la même chaîne rend dans une image relue sur le processeur (essais, captures, vignettes).

## Interface (obv-ui)

- Mode « retenu » léger : arbre de widgets reconstruit seulement quand l'état change ; mise en page en deux passes
  (mesure puis placement), flex et grille.
- Dessin : rectangles arrondis, bordures, ombres et dégradés calculés dans le shader à partir de distances signées
  (net à toutes les échelles), texte par atlas de glyphes ; une seule passe, peu de tracés.
- Thèmes : jetons de conception (couleurs, rayons, espacements, typographie) en clair et en sombre ; aucun texte
  d'explication visible, info-bulles partout.
- Entrées : souris, clavier, molette de précision, stylet ; raccourcis configurables.

## Données (obv-scene)

- Arbre d'assemblage (instances et transformations), pièces (maillage interne compact et, pour la CAO, lien vers la
  forme exacte), matériaux PBR, unités, métadonnées.
- Format interne : positions quantifiées, normales octaédriques, indices locaux 8 bits par grappe ; cache disque par
  fichier pour une seconde ouverture instantanée.

## Conversion (obv-mesh, obv-cad)

Voir D-09. Chaîne : source exacte (CAO) ou reconstruite (STL) → tessellation ou remaillage sous contrainte de
tolérance → contrôle d'écart → écriture.
