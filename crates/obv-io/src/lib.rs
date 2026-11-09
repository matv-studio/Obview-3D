//! Lecture des fichiers 3D vers le modèle interne (D-07). Premier lecteur : glTF binaire (GLB).
//!
//! Le fichier est projeté en mémoire ; chaque primitive est décodée en parallèle.

mod glb;

pub use glb::read_glb;

/// Maillage d'une pièce, positions déjà placées dans le repère de la scène.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut it = self.positions.iter();
        let first = *it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| {
            ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
        }))
    }
}

/// Élément de l'arbre d'assemblage.
#[derive(Clone, Debug)]
pub struct Part {
    pub name: String,
    pub mesh: Mesh,
}

#[derive(Clone, Debug, Default)]
pub struct Model {
    pub parts: Vec<Part>,
    /// Unité des coordonnées en mètres (glTF : 1).
    pub unit_m: f64,
}

impl Model {
    pub fn triangle_count(&self) -> usize {
        self.parts.iter().map(|p| p.mesh.triangle_count()).sum()
    }

    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        self.parts.iter().filter_map(|p| p.mesh.bounds()).reduce(|(a, b), (c, d)| {
            ([a[0].min(c[0]), a[1].min(c[1]), a[2].min(c[2])], [b[0].max(d[0]), b[1].max(d[1]), b[2].max(d[2])])
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("lecture du fichier : {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON glTF : {0}")]
    Json(#[from] serde_json::Error),
    #[error("fichier invalide : {0}")]
    Invalid(String),
}
