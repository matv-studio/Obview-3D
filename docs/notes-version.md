## Téléchargements

- **Windows 10 ou 11 (x64)** : `Obview-3D-<version>-windows-x64-setup.exe`. Installation pour l'utilisateur, sans
  droits d'administrateur (« tous les utilisateurs » au choix), menu Démarrer, « Ouvrir avec » pour les formats
  lus, désinstallation depuis les Paramètres de Windows. Vulkan 1.3 si le pilote l'offre, sinon OpenGL 4.3.
- **macOS 26 ou plus, puce Apple M1 ou plus récente** : `Obview-3D-<version>-macos-arm64.dmg`, ouvrir puis
  glisser « Obview 3D » dans Applications. Vulkan par le pilote KosmicKrisp, livré dans l'application.
- **Linux x86_64** : `obview-3d_<version>_amd64.deb` (Debian 12, Ubuntu 22.04 et plus récents ; menu des
  applications) ou `Obview-3D-<version>-linux-x86_64.AppImage` (toutes distributions, fichier à rendre
  exécutable). Pilote Vulkan (Mesa ou constructeur) ou OpenGL 4.3.

Les versions suivantes s'installent depuis l'application (bouton « Mettre à jour » de la barre du haut, ou
Réglages › Mises à jour).

## Première ouverture

- **Windows** : l'installateur n'est pas signé ; SmartScreen affiche « Windows a protégé votre ordinateur » :
  « Informations complémentaires » puis « Exécuter quand même ». Les mises à jour faites depuis l'application
  ne passent pas par cet avertissement.
- **macOS** : l'application n'est pas signée par un compte développeur Apple. Ouvrir « Obview 3D » une fois,
  fermer le message, puis Réglages Système → Confidentialité et sécurité → section Sécurité → « Ouvrir quand
  même » (bouton affiché environ une heure après la tentative), confirmer par mot de passe ou Touch ID.

## Nouveautés de 0.3.0

- Éclairage : dix ambiances (studio, studio photo, studio chaud, intérieur, extérieur, coucher de soleil,
  lumière dure, contre-jour, néon, chaud et froid), HDRI personnel (.hdr, .exr), exposition, rotation,
  environnement en arrière-plan (flou réglable), teinte ; Shift + glisser avec le bouton droit pour tourner le
  dôme. Onglet « Lighting » du panneau de droite.
- Menu des vues enregistrées : bouton caméra en haut à droite de la vue.
- Fichiers récents : clic sur le nom du fichier dans la barre du haut.
- Bouton de la barre du bas (Ctrl + \) pour masquer ou montrer les deux panneaux ; compteur triangles et
  sommets dans la vue quand le panneau de gauche est masqué.
- Nouveau logo.
- Installateurs Windows, macOS et Linux ; mises à jour détectées et installées depuis l'application (empreinte
  SHA-256 vérifiée), recherche au démarrage au plus une fois par jour, désactivable.
- Thème sombre par défaut, barre de titre de la fenêtre comprise.
- Rotation autour de l'objet par défaut (centre du modèle, ou de la pièce cadrée par Focus) ; la rotation
  autour du point visé reste au choix dans Réglages › Navigation.
- Double-clic sur une pièce dans la vue : liste des pièces ouverte et centrée sur elle. Double-clic dans la
  liste : propriétés de la pièce.

HDRI : Poly Haven (CC0) — Sergej Majboroda, Greg Zaal, Jarod Guest.

## Nouveautés de 0.2.0

- Moteur OpenGL 4.3 de secours quand Vulkan manque, même image que Vulkan ; choix dans Réglages → Affichage.
- Version macOS pour Apple Silicon (Vulkan 1.4 par KosmicKrisp).
- Conversion GLB : compression Draco (fichiers lus par Blender, three.js, Babylon.js) ; l'écart maximal demandé
  est désormais garanti aussi avec la quantification.
- Navigation « Obview » par défaut (gauche : panoramique, droit : orbite, molette : zoom), Blender au choix.
- Transitions de vue, progression de l'ouverture, F pour cadrer la sélection, outil « Focus ».
- Interface en anglais par défaut, français au choix.
- GLB exportés lisibles par Blender.
