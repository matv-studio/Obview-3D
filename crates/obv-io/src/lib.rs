//! Lecture des fichiers 3D vers le modèle interne (D-07) : glTF binaire (GLB), STL (binaire et texte), OBJ,
//! FBX (ufbx).
//!
//! Les fichiers sont projetés en mémoire et décodés en parallèle.

mod fbx;
mod glb;
pub mod normals;
mod obj;
mod stl;

use std::path::Path;

pub use fbx::read_fbx;
pub use glb::read_glb;
pub use obj::{parse_obj, read_obj};
pub use stl::{parse_stl, read_stl};

/// Extensions reconnues (minuscules).
pub const EXTENSIONS: &[&str] = &["glb", "stl", "obj", "fbx"];

/// Lit un fichier d'après son extension.
pub fn read(path: &Path) -> Result<Model, ReadError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "glb" => read_glb(path),
        "stl" => read_stl(path),
        "obj" => read_obj(path),
        "fbx" => read_fbx(path),
        _ => Err(ReadError::Unsupported(ext)),
    }
}

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
    #[error("format non pris en charge : .{0}")]
    Unsupported(String),
}
