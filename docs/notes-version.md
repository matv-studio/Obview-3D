## Téléchargements

- **Windows 10 ou 11 (x64)** : `Obview-3D-<version>-windows-x64.zip`, à extraire, puis lancer « Obview 3D.exe ».
  Vulkan 1.3 si le pilote l'offre, sinon OpenGL 4.3.
- **macOS 26 ou plus, puce Apple M1 ou plus récente** : `Obview-3D-<version>-macos-arm64.zip`, à extraire, puis
  glisser « Obview 3D.app » dans Applications. Vulkan par le pilote KosmicKrisp, livré dans l'application.

## Première ouverture sur macOS

L'application n'est pas signée par un compte développeur Apple. macOS la bloque à la première ouverture :

1. Ouvrir « Obview 3D.app » une fois, puis fermer le message.
2. Réglages Système → Confidentialité et sécurité → section Sécurité → « Ouvrir quand même », puis confirmer
   avec le mot de passe ou Touch ID.

Le bouton reste affiché environ une heure après la tentative d'ouverture. Les ouvertures suivantes sont normales.

## Nouveautés de 0.2.1

- Rotation autour de l'objet par défaut (centre du modèle, ou de la pièce cadrée par Focus) ; la rotation
  autour du point visé reste au choix dans Réglages › Navigation.
- Double-clic sur une pièce dans la vue : liste des pièces ouverte et centrée sur elle. Double-clic dans la
  liste : propriétés de la pièce.

## Nouveautés de 0.2.0

- Moteur OpenGL 4.3 de secours quand Vulkan manque, même image que Vulkan ; choix dans Réglages → Affichage.
- Version macOS pour Apple Silicon (Vulkan 1.4 par KosmicKrisp).
- Conversion GLB : compression Draco (fichiers lus par Blender, three.js, Babylon.js) ; l'écart maximal demandé
  est désormais garanti aussi avec la quantification.
- Navigation « Obview » par défaut (gauche : panoramique, droit : orbite, molette : zoom), Blender au choix.
- Transitions de vue, progression de l'ouverture, F pour cadrer la sélection, outil « Focus ».
- Interface en anglais par défaut, français au choix.
- GLB exportés lisibles par Blender.
