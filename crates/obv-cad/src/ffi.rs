//! Pont `cxx` vers le lecteur C++ (`cpp/reader.cpp`).

use obv_io::{Progress, Stage};

/// Avancement vu du C++ : étape, fraction, demande d'arrêt.
pub struct CadProgress<'a>(pub &'a Progress);

impl CadProgress<'_> {
    /// `stage` : 0 lecture, 1 conversion, 2 maillage ; `fraction` < 0 : indéterminée.
    fn report(&self, stage: u8, fraction: f64) {
        let s = match stage {
            0 => Stage::Read,
            1 => Stage::Transfer,
            _ => Stage::Mesh,
        };
        if self.0.get().0 != s {
            self.0.stage(s);
        }
        if fraction >= 0.0 {
            self.0.set(fraction as f32);
        }
    }

    fn cancelled(&self) -> bool {
        self.0.cancelled()
    }
}

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

    extern "Rust" {
        type CadProgress<'a>;
        fn report(self: &CadProgress<'_>, stage: u8, fraction: f64);
        fn cancelled(self: &CadProgress<'_>) -> bool;
    }

    unsafe extern "C++" {
        include!("obv-cad/cpp/reader.h");

        /// Lit un fichier STEP (`iges` faux) ou IGES et maille ses solides ; s'arrête au plus tôt quand
        /// `progress` demande l'arrêt (résultat alors sans objet).
        fn read_cad(path: &str, iges: bool, options: &CadOptions, progress: &CadProgress<'_>) -> CadResult;

        /// Écrit l'assemblage d'essai (deux instances nommées d'un cube de 10 mm, la seconde décalée de 20 mm
        /// en X et rouge) : jeu d'essai libre, produit à la volée plutôt que versé au dépôt.
        #[allow(dead_code)]
        fn write_test_step(path: &str) -> bool;
    }
}
