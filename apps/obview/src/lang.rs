//! Traductions anglaises de l'application (D-29) : clé = texte français écrit dans le code.
//! L'anglais est la langue par défaut ; le réglage Apparence › Langue choisit le français.

use std::borrow::Cow;

use obv_ui::i18n;

/// Enregistre la table (une fois par processus).
pub fn init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| i18n::register(EN));
}

/// Nom de pièce ou de matériau numéroté par un lecteur (« Pièce 12 », « Couleur 3 », « Matériau 2 ») dans la
/// langue active ; les autres noms, venus du fichier, restent tels quels.
pub fn part_name(name: &str) -> Cow<'_, str> {
    if i18n::is_fr() {
        return Cow::Borrowed(name);
    }
    for (fr, en) in [("Pièce ", "Part "), ("Couleur ", "Color "), ("Matériau ", "Material ")] {
        if let Some(n) = name.strip_prefix(fr)
            && !n.is_empty()
            && n.bytes().all(|b| b.is_ascii_digit())
        {
            return Cow::Owned(format!("{en}{n}"));
        }
    }
    Cow::Borrowed(name)
}

/// Texte français → texte anglais.
pub const EN: &[(&str, &str)] = &[
    // Barre du haut
    ("Ouvrir un fichier", "Open a file"),
    ("Thème clair", "Light theme"),
    ("Thème sombre", "Dark theme"),
    ("Réglages", "Settings"),
    ("Convertir", "Convert"),
    ("Convertir en maillage optimisé (GLB, OBJ, FBX)", "Convert to an optimized mesh (GLB, OBJ, FBX)"),
    // Panneaux
    ("Pièces", "Parts"),
    ("Vues", "Views"),
    ("Rechercher", "Search"),
    ("Propriétés", "Properties"),
    ("Focus", "Focus"),
    ("Géométrie", "Geometry"),
    ("Triangles", "Triangles"),
    ("Sommets", "Vertices"),
    ("Largeur", "Width"),
    ("Hauteur", "Height"),
    ("Profondeur", "Depth"),
    ("Apparence", "Appearance"),
    ("Couleur", "Color"),
    ("Matériau", "Material"),
    ("Par défaut", "Default"),
    ("Sans nom", "Unnamed"),
    ("Modèle", "Model"),
    ("Fichier", "File"),
    ("Orienter la vue", "Orient the view"),
    // Vues
    ("Standard", "Standard"),
    ("Enregistrées", "Saved"),
    ("Nom", "Name"),
    ("Supprimer la vue", "Delete view"),
    ("Clic : afficher · double-clic : renommer", "Click: show · double-click: rename"),
    ("Enregistrer la vue", "Save view"),
    ("Vue actuelle", "Current view"),
    ("Vue", "View"),
    ("Avant", "Front"),
    ("Arrière", "Back"),
    ("Gauche", "Left"),
    ("Droite", "Right"),
    ("Dessus", "Top"),
    ("Dessous", "Bottom"),
    ("Isométrique", "Isometric"),
    // Outils
    ("Sélection", "Select"),
    ("Mesurer", "Measure"),
    ("Coupe", "Section"),
    ("Vue éclatée", "Exploded view"),
    ("Vues de rendu", "Render views"),
    ("Perspective ou orthographique", "Perspective or orthographic"),
    ("Capture", "Screenshot"),
    ("Distance", "Distance"),
    ("Écart", "Delta"),
    ("Effacer", "Clear"),
    ("Effacer toutes les mesures (Suppr)", "Clear all measurements (Del)"),
    ("Position X", "X position"),
    ("Position Y", "Y position"),
    ("Position Z", "Z position"),
    ("Écartement", "Spread"),
    ("Glisser pour déplacer le plan", "Drag to move the plane"),
    ("Taille", "Size"),
    ("Arrière-plan", "Background"),
    ("Fond", "Solid"),
    ("Transparent", "Transparent"),
    ("Enregistrer l'image", "Save image"),
    // Vues de rendu
    ("Éclairé", "Lit"),
    ("Normales", "Normals"),
    ("Topologie", "Topology"),
    ("Rayons X", "X-ray"),
    ("Arêtes", "Edges"),
    ("Toutes les vues", "All views"),
    ("triangles", "triangles"),
    ("sommets", "vertices"),
    // Ouverture
    ("Lecture", "Reading"),
    ("Import", "Importing"),
    ("Maillage", "Meshing"),
    ("Préparation", "Preparing"),
    ("Annuler l'ouverture", "Cancel opening"),
    ("Impossible d'ouvrir", "Couldn't open"),
    ("Capture impossible", "Couldn't save the screenshot"),
    ("Ouvrir", "Open"),
    ("Modèles 3D", "3D models"),
    ("Tous les fichiers", "All files"),
    ("Image PNG", "PNG image"),
    ("modele", "model"),
    ("vue", "view"),
    // Conversion
    ("Format", "Format"),
    ("Fidélité", "Fidelity"),
    ("Forme exacte, fichier plus lourd", "Exact shape, larger file"),
    (
        "Gauche : la forme d'abord. Droite : le moins de triangles possible",
        "Left: shape first. Right: as few triangles as possible",
    ),
    ("Performance", "Performance"),
    ("Fichier léger, forme approchée", "Light file, approximate shape"),
    ("Densité", "Density"),
    ("Écart maximal", "Max deviation"),
    (
        "Distance maximale garantie entre la forme d'origine et le résultat",
        "Guaranteed maximum distance between the original shape and the result",
    ),
    ("Silhouette", "Silhouette"),
    (
        "Arêtes vives et contours plus fidèles, au prix de quelques triangles",
        "Sharper edges and outlines, at the cost of a few triangles",
    ),
    ("Aucune", "None"),
    ("Quantifiée", "Quantized"),
    ("Compression", "Compression"),
    ("Flottants : lisible partout", "Floats: readable everywhere"),
    (
        "KHR_mesh_quantization : environ deux fois plus léger ; Blender, Unity, Unreal, three.js, Babylon.js",
        "KHR_mesh_quantization: about half the size; Blender, Unity, Unreal, three.js, Babylon.js",
    ),
    (
        "EXT_meshopt_compression : très léger, le plus rapide à décoder, pour le web (three.js, Babylon.js) ; Blender ne le lit pas",
        "EXT_meshopt_compression: very small, fastest to decode, for the web (three.js, Babylon.js); Blender can't read it",
    ),
    (
        "KHR_draco_mesh_compression : le plus compact ; Blender, three.js, Unity (glTFast), Babylon.js ; décodage plus lent",
        "KHR_draco_mesh_compression: most compact; Blender, three.js, Unity (glTFast), Babylon.js; slower to decode",
    ),
    ("Réduction", "Reduction"),
    ("Écart mesuré", "Measured deviation"),
    ("Écart garanti", "Guaranteed deviation"),
    ("Exact", "Exact"),
    ("Dossier", "Folder"),
    ("Montrer le fichier", "Show the file"),
    ("Ouvrir le résultat", "Open result"),
    ("Ouvrir le fichier produit pour le comparer", "Open the output file to compare it"),
    ("Annuler", "Cancel"),
    ("Conversion…", "Converting…"),
    ("Choisir le fichier de sortie et convertir", "Choose the output file and convert"),
    ("Ko", "KB"),
    ("Mo", "MB"),
    // Réglages
    ("Navigation", "Navigation"),
    ("Affichage", "Display"),
    ("Barre d'outils", "Toolbar"),
    ("Mises à jour", "Updates"),
    ("Mettre à jour", "Update"),
    ("Installation", "Installing"),
    ("Rechercher une version", "Look for a newer version"),
    ("Vérifier", "Check"),
    ("À jour", "Up to date"),
    ("Vérification", "Checking"),
    ("Notes", "Notes"),
    ("Notes de version", "Release notes"),
    ("Échec", "Failed"),
    ("Réessayer", "Retry"),
    ("Version", "Version"),
    ("Dernière version", "Latest version"),
    ("Rechercher au démarrage", "Check at startup"),
    ("Interface", "Interface"),
    ("Langue", "Language"),
    ("Thème", "Theme"),
    ("Sombre", "Dark"),
    ("Clair", "Light"),
    ("Système", "System"),
    ("Couleur d'accent", "Accent color"),
    ("Couleur personnalisée", "Custom color"),
    ("Étendue de l'accent", "Accent scope"),
    ("Minimale", "Minimal"),
    ("Étendue", "Extended"),
    (
        "Minimale : focus et contour de la vue. Standard : + action principale, interrupteurs, sélection. Étendue : + onglets, choix, lignes actives",
        "Minimal: focus and view outline. Standard: + primary action, switches, selection. Extended: + tabs, choices, active rows",
    ),
    ("Choisir d'abord une couleur d'accent", "Choose an accent color first"),
    ("Animations", "Animations"),
    ("Souris", "Mouse"),
    ("Schéma", "Scheme"),
    ("ou", "or"),
    ("Molette", "Wheel"),
    ("Orbite", "Orbit"),
    ("Panoramique", "Pan"),
    ("Zoom", "Zoom"),
    ("Maj", "Shift"),
    ("Milieu", "Middle"),
    ("Droit", "Right"),
    ("Comportement", "Behavior"),
    ("Inverser le zoom", "Invert zoom"),
    ("Zoomer vers le pointeur", "Zoom toward the pointer"),
    ("Pivoter autour du point visé", "Orbit around the point under the pointer"),
    ("Vitesse du zoom", "Zoom speed"),
    ("Vue 3D", "3D view"),
    ("Unités", "Units"),
    ("Millimètres", "Millimeters"),
    ("Centimètres", "Centimeters"),
    ("Mètres", "Meters"),
    ("Pouces", "Inches"),
    ("po", "in"),
    ("Gizmo de navigation", "Navigation gizmo"),
    ("Axes", "Axes"),
    ("Cube", "Cube"),
    ("Trièdre", "Triad"),
    ("Aucun", "None"),
    ("Boules d'axe, comme Blender", "Axis balls, like Blender"),
    (
        "Cube à faces nommées : faces, arêtes et coins cliquables",
        "Cube with named faces: faces, edges and corners are clickable",
    ),
    ("Trièdre compact, comme les logiciels de CAO", "Compact triad, like CAD software"),
    ("Pas de gizmo", "No gizmo"),
    ("Barre du bas", "Bottom bar"),
    ("Rendu", "Rendering"),
    ("Moteur", "Engine"),
    ("Actif", "Active"),
    ("Auto", "Auto"),
    ("Vulkan, OpenGL en secours", "Vulkan, OpenGL as fallback"),
    ("Vulkan 1.3 ou plus récent", "Vulkan 1.3 or later"),
    ("OpenGL 4.3 ou plus récent", "OpenGL 4.3 or later"),
    ("OpenGL indisponible", "OpenGL unavailable"),
    ("Vulkan indisponible", "Vulkan unavailable"),
    ("Rétablir", "Reset"),
    ("Valeurs par défaut", "Default values"),
    ("Terminé", "Done"),
    ("Fermer les réglages", "Close settings"),
    // Messages des lecteurs (début connu, détail gardé)
    ("lecture STEP / IGES absente de cette construction", "STEP / IGES reading is not part of this build"),
    ("format non pris en charge :", "unsupported format:"),
    ("ouverture annulée", "opening canceled"),
    ("lecture du fichier :", "reading the file:"),
    ("JSON glTF :", "glTF JSON:"),
    ("fichier invalide :", "invalid file:"),
    ("lecture :", "reading:"),
    (
        "lecture refusée par OpenCASCADE (fichier illisible ou incomplet)",
        "OpenCASCADE refused the file (unreadable or incomplete)",
    ),
    ("chemin non représentable en UTF-8", "path not representable in UTF-8"),
    ("OBJ : ligne illisible :", "OBJ: unreadable line:"),
    ("OBJ : indice hors limites", "OBJ: index out of range"),
    ("OBJ non UTF-8", "OBJ is not UTF-8"),
    ("STL binaire tronqué", "truncated binary STL"),
    ("STL texte : nombre de sommets non multiple de 3", "text STL: vertex count is not a multiple of 3"),
    ("STL texte non UTF-8", "text STL is not UTF-8"),
    ("STL trop court", "STL too short"),
    ("coordonnées non finies", "non-finite coordinates"),
    ("sommet illisible :", "unreadable vertex:"),
    ("mode de compression inconnu :", "unknown compression mode:"),
    ("primitive sans POSITION", "primitive without POSITION"),
    ("bloc JSON absent", "missing JSON chunk"),
    ("graphe de nœuds cyclique", "cyclic node graph"),
    ("données meshoptimizer illisibles", "unreadable meshoptimizer data"),
    ("Draco :", "Draco:"),
    ("compression Draco absente de cette construction", "Draco compression is not part of this build"),
    ("accesseur clairsemé non géré", "sparse accessor not supported"),
    ("accesseur hors de sa vue", "accessor outside its view"),
    ("vue hors du tampon", "view outside the buffer"),
    ("tampon absent", "missing buffer"),
];

/// Textes passés à `tr` par une variable (non relevés par l'essai de complétude) : listés ici pour être vérifiés.
#[cfg(test)]
const INDIRECT: &[&str] = &[
    "Mises à jour",
    "Avant",
    "Arrière",
    "Gauche",
    "Droite",
    "Dessus",
    "Dessous",
    "Isométrique",
    "Impossible d'ouvrir",
    "Capture impossible",
    "OpenGL indisponible",
    "Vulkan indisponible",
    "Vulkan, OpenGL en secours",
    "Vulkan 1.3 ou plus récent",
    "OpenGL 4.3 ou plus récent",
    "Flottants : lisible partout",
    "KHR_mesh_quantization : environ deux fois plus léger ; Blender, Unity, Unreal, three.js, Babylon.js",
    "EXT_meshopt_compression : très léger, le plus rapide à décoder, pour le web (three.js, Babylon.js) ; Blender ne le lit pas",
    "KHR_draco_mesh_compression : le plus compact ; Blender, three.js, Unity (glTFast), Babylon.js ; décodage plus lent",
    "Bleu",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Littéraux passés à `tr(…)` dans le code (hors essais) : appel et arguments, parenthèses équilibrées.
    fn tr_literals(src: &str) -> Vec<String> {
        let code = src.split("#[cfg(test)]").next().unwrap_or(src);
        let code: String = code.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n");
        let b = code.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while let Some(k) = code[i..].find("tr(") {
            let start = i + k;
            i = start + 3;
            if start > 0 && (b[start - 1].is_ascii_alphanumeric() || b[start - 1] == b'_') {
                continue;
            }
            let mut depth = 1;
            let mut j = i;
            while j < b.len() && depth > 0 {
                match b[j] {
                    b'"' => {
                        let s = j + 1;
                        j += 1;
                        while j < b.len() && b[j] != b'"' {
                            if b[j] == b'\\' {
                                j += 1;
                            }
                            j += 1;
                        }
                        out.push(code[s..j].replace("\\'", "'"));
                    }
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
        }
        out
    }

    #[test]
    fn chaque_texte_a_sa_traduction() {
        init();
        let dirs = [
            concat!(env!("CARGO_MANIFEST_DIR"), "/src"),
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/obv-ui/src"),
        ];
        let mut missing = Vec::new();
        let mut count = 0;
        for d in dirs {
            for e in std::fs::read_dir(d).unwrap() {
                let p = e.unwrap().path();
                if p.extension().is_some_and(|x| x == "rs") {
                    for t in tr_literals(&std::fs::read_to_string(&p).unwrap()) {
                        count += 1;
                        if !i18n::has_translation(&t) {
                            missing.push(format!("{} : {t}", p.file_name().unwrap().to_string_lossy()));
                        }
                    }
                }
            }
        }
        for t in INDIRECT {
            if !i18n::has_translation(t) {
                missing.push(format!("(indirect) {t}"));
            }
        }
        assert!(count > 150, "relevé trop maigre : {count}");
        assert!(missing.is_empty(), "sans traduction :\n{}", missing.join("\n"));
    }

    #[test]
    fn noms_numerotes() {
        i18n::set_lang(obv_ui::Lang::En);
        assert_eq!(part_name("Pièce 12"), "Part 12");
        assert_eq!(part_name("Couleur 3"), "Color 3");
        assert_eq!(part_name("Pièce A"), "Pièce A");
        i18n::set_lang(obv_ui::Lang::Fr);
        assert_eq!(part_name("Pièce 12"), "Pièce 12");
    }
}
