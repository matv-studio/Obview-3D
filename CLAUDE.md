# Obview 3D · mémoire de travail de Claude

Visionneuse 3D de bureau, très rapide, pour les fichiers CAO et maillages (STEP, IGES, STL, OBJ, glTF/GLB, FBX…),
avec en bonus un bouton de conversion vers des maillages optimisés (GLB, OBJ, FBX).
Dépôt : https://github.com/matv-studio/Obview-3D (public, propriétaire matv-studio). Projet claude.ai « Apps interne ».

## Reprise (à faire au début de chaque nouveau chat)

1. Lire ce fichier en entier, puis `docs/ETAT.md` (où en est le travail) et `docs/DECISIONS.md` (pourquoi).
2. Vérifier le compte GitHub : `gh api user --jq .login` doit répondre `Alunites` ; droits d'écriture sur
   `matv-studio/Obview-3D` (`gh api repos/matv-studio/Obview-3D --jq .permissions`).
3. Remettre l'environnement en place (section « Environnement »), puis lancer `tools/verify.sh`.
4. Reprendre au point « Prochaines étapes » de `docs/ETAT.md`.
5. Ne relire les anciens chats que si ces fichiers ne suffisent pas.

## Règles de l'utilisateur (permanentes)

Réponses
- Ton neutre, technique, direct ; ni compliment, ni validation, ni formule d'introduction.
- Plusieurs solutions : les lister avec leurs contreparties, sans en désigner une comme « la bonne ».
- Avant tout conseil technique : recherche web, croisement avec les fichiers et captures fournis, cohérence ;
  jamais de supposition ; sources en fin de réponse.
- Prévenir avant un long travail, donner des nouvelles à chaque étape ; en fin de travail : résultat, captures,
  une suite possible, pas de récit des étapes.

Produit (consignes du 02/11 et du 09/11/2026)
- **Aucun raccourci** : « l'outil le plus solide et performant que tu es capable d'écrire », même si c'est plus long.
  On ne choisit pas une brique pour sa facilité d'intégration quand on peut faire mieux.
- Les licences ne filtrent pas les choix pour l'instant (« on avisera après ») ; la question de la distribution est
  reportée (D-11).
- Ergonomie primordiale, au niveau des meilleurs outils du domaine (Autodesk Viewer, CAD Assistant, eDrawings,
  F3D, Sketchfab, Figma pour la finition de l'interface).
- Peu de texte dans l'interface : **aucune phrase d'explication visible**, des info-bulles.
- Une demande proche d'une fonction existante s'intègre à cette fonction (le signaler) ; deux fonctions distinctes
  restent deux outils distincts.
- Chaque écran vérifié visuellement **dans les deux thèmes**, captures envoyées.
- Quand une limite bloque l'utilisateur, créer l'outil qui la lève.

Dépôt
- Auteur des commits : `Claude <noreply@anthropic.com>` (demande de l'utilisateur du 09/11/2026 : « écrit sous ton
  nom, pas Alunites »). Les envois passent par le compte GitHub Alunites (collaborateur de matv-studio).
- Messages de commit en français : titre court, corps en phrases citant les D-xx ; dernières lignes :
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` puis `Claude-Session: <adresse du chat>`.
- Travail sur `main` tant que l'utilisateur ne demande pas de branches.
- API GitHub par `gh api` (REST) ; GraphQL refusé dans les sessions (`gh workflow run` passe par GraphQL : utiliser
  `gh api -X POST .../dispatches`).
- Jamais de modification des réglages, secrets ou clés du dépôt. Versions : brouillons préparés par Claude,
  publiés par l'utilisateur.
- Aucune donnée personnelle ni fichier client dans git. Les fichiers d'essai réels (montre Santos-Dumont,
  `WSSA0089.stp`, export Rhino `santos_dumont.glb`) restent hors dépôt : jeux d'essai du dépôt fictifs ou libres.
- Avant chaque envoi : `tools/verify.sh` (format, clippy, tests, rendus de référence) ; puis suivre la CI du dernier
  commit et traiter ses échecs.
- Pas de dépense (abonnement, crédits, construction lourde) sans accord.

## Environnement (session Linux de Claude)

- Rust stable (`rust-toolchain.toml`).
- Vulkan logiciel **lavapipe** (Mesa ≥ 25, Vulkan 1.4, mesh et task shaders, atomiques 64 bits) :
  `apt-get install -y mesa-vulkan-drivers glslang-tools vulkan-tools spirv-tools libvulkan-dev vulkan-validationlayers xvfb`.
  Vérifier : `vulkaninfo --summary` doit montrer `llvmpipe`.
- Shaders : GLSL compilés en SPIR-V par `glslangValidator` au moment de la construction (`build.rs`).
- Les essais de rendu tournent **hors écran** (aucune fenêtre) : `cargo test` suffit, captures écrites en PNG.
- La machine n'a que 2 cœurs et ~7 Go : construire en `--release` seulement quand nécessaire.

## Carte du dépôt

- `crates/obv-gpu` : couche Vulkan (instance, appareil, mémoire, transferts, pipelines, cibles hors écran, fenêtre).
- `crates/obv-render` : rendu de la scène (géométrie en grappes, tri des grappes, ombrage, arêtes, sélection).
- `crates/obv-ui` : bibliothèque d'interface maison (mise en page, widgets, texte, thèmes, info-bulles).
- `crates/obv-scene`, `crates/obv-io`, `crates/obv-cad`, `crates/obv-mesh` : voir `docs/ARCHITECTURE.md`.
- `apps/obview` : l'application.
- `docs/` : `ETAT.md`, `DECISIONS.md`, `ARCHITECTURE.md`.
- `tools/` : scripts de vérification.

## Pièges connus

- `Id::with(clé)` : les entiers de tout type donnent la même clé (`IdPart`), mais une chaîne et un entier non.
- Un pipeline dessiné dans une passe qui a une profondeur doit déclarer ce format (`depth_format`), même sans test
  de profondeur : sinon erreur de validation VUID-vkCmdDraw-dynamicRenderingUnusedAttachments-08914.
- L'interface est dessinée après la 3D : aucun fond sur les nœuds qui recouvrent la vue (la couleur de fond de la
  fenêtre vient de l'effacement de la cible).
- Captures : `cargo test -p obview --test shell` (références dans `apps/obview/tests/reference`,
  `OBV_UPDATE_REFERENCE=1` pour les réécrire) ; fichier local à l'échelle 2 :
  `OBV_SAMPLE_GLB=<fichier.glb> OBV_SELECT=<n> cargo test -p obview --test shell captures_fichier_local -- --ignored`.
  Images dans `target/tmp/rendus/`.
