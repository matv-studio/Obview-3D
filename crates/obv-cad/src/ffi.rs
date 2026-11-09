//! Pont `cxx` vers le lecteur C++ (`cpp/reader.cpp`).

#[cxx::bridge(namespace = "obv")]
pub mod bridge {
    /// Triangles consécutifs d'une pièce partageant un style (couleur de surface du fichier).
    struct CadGroup {
        first: u32,
        count: u32,
        /// RGBA linéaire.
        color: [f32; 4],
        has_color: bool,
    }

    /// Pièce maillée, dans le repère de l'assemblage (transformation de l'instance appliquée).
    struct CadPart {
        name: String,
        /// Chemin dans l'assemblage, noms séparés par « / ».
        path: String,
        positions: Vec<f32>,
        normals: Vec<f32>,
        indices: Vec<u32>,
        groups: Vec<CadGroup>,
    }

    /// Réglages du maillage des surfaces exactes.
    struct CadOptions {
        /// Flèche maximale, en fraction de la diagonale de la boîte englobante du modèle.
        linear: f64,
        /// Écart angulaire maximal entre deux normales voisines (radians).
        angular: f64,
    }

    struct CadResult {
        parts: Vec<CadPart>,
        /// Mètres par unité des coordonnées.
        unit_m: f64,
        /// Vide : lecture réussie.
        error: String,
    }

    unsafe extern "C++" {
        include!("obv-cad/cpp/reader.h");

        /// Lit un fichier STEP (`iges` faux) ou IGES et maille ses solides.
        fn read_cad(path: &str, iges: bool, options: &CadOptions) -> CadResult;

        /// Écrit l'assemblage d'essai (deux instances nommées d'un cube de 10 mm, la seconde décalée de 20 mm
        /// en X et rouge) : jeu d'essai libre, produit à la volée plutôt que versé au dépôt.
        #[allow(dead_code)]
        fn write_test_step(path: &str) -> bool;
    }
}
