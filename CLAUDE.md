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
- Écran de référence : **16:9** (captures en 1920 × 1080, vérification en 2560 × 1440).
- Navigation 3D : schémas des logiciels courants au choix, **schéma « Obview » par défaut** (gauche :
  panoramique, droit : orbite, molette ou Alt + droit : zoom ; D-29, Blender au choix, D-13), rotation autour
  de l'objet cadré par défaut (D-31). Couleur d'accent
  au choix (D-14). **Anglais par défaut**, français au choix (D-29) : tout texte d'interface passe par
  `tr("texte français")` avec sa traduction dans `apps/obview/src/lang.rs` (ou `obv_ui::i18n::OWN`) ; un essai
  le vérifie. Vues de rendu présentées comme la visionneuse web Marmoset (D-15).
- Le format `.mview` n'est pas pris en charge (décision du 09/11/2026).
- **Ne pas mettre à jour la note de passation** (PC, documents du projet claude.ai) sauf demande de
  l'utilisateur. La mémoire du dépôt (ce fichier, `docs/`) reste tenue à jour.

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
- OpenCASCADE (D-27) : `tools/occt.sh /opt/occt` (sources clonées depuis GitHub, ~3 h sur 2 cœurs ; dossier
  déjà construit : rien à refaire), puis `export OCCT_DIR=/opt/occt` avant `cargo` ; sans cette variable,
  la lecture STEP / IGES est absente de la construction.
- Draco (D-30) : `tools/draco.sh /opt/draco` (quelques minutes), puis `export DRACO_DIR=/opt/draco` ; sans elle,
  la compression Draco est absente. `tools/verify.sh` se lance avec `OCCT_DIR` et `DRACO_DIR`.
- OpenGL sans fenêtre pour les essais (D-30) : EGL et llvmpipe (`libegl1 libegl-mesa0 libgl1-mesa-dri`).
- Vérifier la compilation pour macOS sans Mac : `rustup target add aarch64-apple-darwin`, puis
  `cargo clippy --workspace --all-targets --target aarch64-apple-darwin` sans `OCCT_DIR` ni `DRACO_DIR`, avec
  `CC_aarch64_apple_darwin` et `CXX_aarch64_apple_darwin` pointant vers un compilateur factice qui écrit des
  objets vides (`clang -target arm64-apple-macos11 -x c /dev/null -c -o <sortie>`, le reste passé à clang).

## Carte du dépôt

- `crates/obv-gpu` : couche Vulkan (instance, appareil, mémoire, transferts, pipelines, cibles hors écran, fenêtre).
- `crates/obv-render` : rendu de la scène (géométrie en grappes, tri des grappes, ombrage, arêtes, sélection).
- `crates/obv-ui` : bibliothèque d'interface maison (mise en page, widgets, texte, thèmes, info-bulles).
- `crates/obv-cad` : lecture STEP / IGES (pont `cxx` vers OpenCASCADE, `cpp/reader.cpp`).
- `crates/obv-draco` : pont `cxx` vers Draco (encodage, décodage).
- `crates/obv-gl` : moteur OpenGL 4.3 de secours (scène, sélection, interface, cible, contexte EGL sans fenêtre).
- `crates/obv-build` : compilation des nuanceurs (SPIR-V, et variantes OpenGL `*.gl` pour obv-render et obv-ui).
- `crates/obv-io`, `crates/obv-mesh` : lecteurs des maillages, moteur de conversion ; voir `docs/ARCHITECTURE.md`.
- `apps/obview` : l'application.
- `docs/` : `ETAT.md`, `DECISIONS.md`, `ARCHITECTURE.md`.
- `tools/` : vérification (`verify.sh`), OpenCASCADE (`occt.sh`), Draco (`draco.sh`), SDK Vulkan
  (`macos-sdk.sh`, `linux-sdk.sh`), Inno Setup (`inno.ps1`), paquets (`macos-app.sh`, `macos-dmg.sh`,
  `linux-package.sh` ; `APPIMAGETOOL=none` : .deb seul, l'outil AppImage téléchargé est refusé dans la session).
- `packaging/` : installateur Windows (`windows/obview.iss`), types MIME Linux.
- Éclairage (D-33) : préfiltrage `crates/obv-render/src/light.rs` (HDRI dans `crates/obv-render/assets/hdri`,
  crédits `CREDITS.md`), envoi au GPU `env_gpu.rs` (Vulkan) et `obv-gl/src/scene.rs`, nuanceurs `light.glsl`,
  `light_block.glsl` ; onglet `apps/obview/src/lighting.rs`. Captures : `OBV_LIGHT=<clé>` (studio, studio-photo,
  studio-warm, interior, outdoor, sunset, hard, rim, neon, warm-cool), `OBV_LIGHT_BG=1` (fond environnement).
- Fichiers récents : `apps/obview/src/recent.rs`.
- Mises à jour : `apps/obview/src/update.rs` (essai réseau : `cargo test -p obview --lib recherche_reelle --
  --ignored`).
- `.github/workflows/` : `ci.yml` (Linux lavapipe et llvmpipe, Windows, macOS 26 arm64),
  `version.yml` (installateurs Windows, macOS, Linux en brouillon, même numéro remplacé ; notes dans
  `docs/notes-version.md`). CI : job `linux-paquets` sur Ubuntu 22.04.

## Pièges connus

- `Id::with(clé)` : les entiers de tout type donnent la même clé (`IdPart`), mais une chaîne et un entier non.
- Un pipeline dessiné dans une passe qui a une profondeur doit déclarer ce format (`depth_format`), même sans test
  de profondeur : sinon erreur de validation VUID-vkCmdDraw-dynamicRenderingUnusedAttachments-08914.
- L'interface est dessinée après la 3D : aucun fond sur les nœuds qui recouvrent la vue (la couleur de fond de la
  fenêtre vient de l'effacement de la cible).
- Constantes poussées : 128 octets au plus (minimum garanti ; pilotes AMD). `gl_PrimitiveID` dans un fragment exige
  `geometryShader` : passer l'indice du triangle depuis le nuanceur de sommets.
- Fenêtre sous Xvfb : `Xvfb :99 -screen 0 1920x1080x24 &`, `DISPLAY=:99 XDG_CONFIG_HOME=/tmp/obvcfg
  target/debug/obview --validation &`, puis `xdotool search --name "Obview 3D"` et `xdotool windowfocus <id>`
  (sans gestionnaire de fenêtres, le clavier n'arrive pas sinon) ; capture : `import -window root fichier.png`.
  `OBV_DEBUG_KEYS=1` affiche les touches reçues ; `OBV_DEBUG_LOAD=1` les étapes d'une ouverture et leur durée.
  Moteur OpenGL : `--gl` (ou `OBV_RENDERER=opengl`), `--vulkan` ; `OBV_GL_DEBUG=1` affiche les messages du
  pilote ; `OBV_GL_NO_CLIP_CONTROL=1` (toute valeur, même vide) force le chemin sans `glClipControl` ;
  `OBV_REQUIRE_GL=1` fait échouer les essais OpenGL au lieu de les sauter (CI).
  Sous lavapipe, une image d'un gros modèle prend plusieurs centaines de ms : les transitions y semblent
  sautées et des clics rapprochés tombent dans la même image (les essais de capture prennent les transitions
  à mi-course par le champ `settle`).
- Icônes : `crates/obv-ui/assets/icons/*.svg` (grille 16, `stroke-width="1.5"` remplacé au rendu) ; ajouter le
  nom dans `crates/obv-ui/src/icon_list.rs`. Geist n'a ni U+202F ni U+2009 (espace synthétique dans `text.rs`).
- Captures : `cargo test -p obview --test shell` (références dans `apps/obview/tests/reference`,
  `OBV_UPDATE_REFERENCE=1` pour les réécrire) ; fichier local à l'échelle 2 :
  `OBV_SAMPLE=<fichier> OBV_SELECT=<n> cargo test -p obview --test shell captures_fichier_local -- --ignored`
  (tout format lisible, STEP compris avec `OCCT_DIR`).
  Images dans `target/tmp/rendus/`.
  Autre taille : `OBV_SIZE=2560x1440 OBV_SCALE=1 OBV_PANELS=1`.
