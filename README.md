# Obview 3D

Visionneuse 3D de bureau pour les fichiers CAO et les maillages (STEP, IGES, STL, OBJ, glTF/GLB, FBX), pensée pour
ouvrir et manipuler des modèles de plusieurs centaines de Mo, avec un bouton de conversion vers des maillages
optimisés.

État : socle en construction (voir `docs/ETAT.md`).

- Rendu : Vulkan direct, mesh shaders, géométrie en grappes avec niveaux de détail continus.
- Interface : bibliothèque maison dessinée dans la même image que la 3D.
- CAO : OpenCASCADE pour les surfaces exactes.

Construire : Rust stable, `glslangValidator` (paquet `glslang-tools` sous Linux, SDK Vulkan sous Windows), puis
`cargo build --release`. Essais : `tools/verify.sh` (Vulkan logiciel lavapipe suffisant).

Tous droits réservés.
