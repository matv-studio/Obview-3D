//! Lecture des fichiers 3D vers le modèle interne (D-07) : glTF binaire (GLB), STL (binaire et texte), OBJ,
//! FBX (ufbx).
//!
//! Les fichiers sont projetés en mémoire et décodés en parallèle.

mod fbx;
mod glb;
pub mod normals;
mod obj;
mod progress;
mod stl;

use std::path::Path;

pub use fbx::read_fbx;
pub use glb::read_glb;
pub use obj::{parse_obj, read_obj};
pub use progress::{Progress, Stage};
pub use stl::{parse_stl, read_stl};

/// Extensions reconnues (minuscules).
pub const EXTENSIONS: &[&str] = &["glb", "gltf", "stl", "obj", "fbx"];

/// Lit un fichier d'après son extension.
pub fn read(path: &Path) -> Result<Model, ReadError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "glb" | "gltf" => read_glb(path),
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
    /// Coordonnées de texture (vide si le fichier n'en a pas ; sinon une par sommet).
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    /// Plages d'indices par matériau, consécutives et dans l'ordre (vide : un seul groupe, sans matériau).
    pub submeshes: Vec<SubMesh>,
}

/// Plage d'indices d'un même matériau.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubMesh {
    /// Premier indice (multiple de 3) et nombre d'indices.
    pub first: u32,
    pub count: u32,
    /// Indice dans `Model::materials`.
    pub material: Option<u32>,
}

/// Matériau physique (métal-rugosité, comme glTF). Couleurs en linéaire.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    pub base_color_texture: Option<u32>,
    pub metallic_roughness_texture: Option<u32>,
    pub normal_texture: Option<u32>,
    pub occlusion_texture: Option<u32>,
    pub emissive_texture: Option<u32>,
    pub double_sided: bool,
    /// Transparence par mélange (sinon opaque).
    pub blend: bool,
}

impl Default for Material {
    fn default() -> Self {
        Material {
            name: String::new(),
            base_color: [1.0; 4],
            metallic: 0.0,
            roughness: 0.5,
            emissive: [0.0; 3],
            base_color_texture: None,
            metallic_roughness_texture: None,
            normal_texture: None,
            occlusion_texture: None,
            emissive_texture: None,
            double_sided: false,
            blend: false,
        }
    }
}

/// Image encodée telle qu'elle est arrivée (PNG, JPEG…), jamais recompressée sans raison.
#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    pub name: String,
    /// Type MIME (`image/png`, `image/jpeg`, `image/webp`…).
    pub mime: String,
    pub data: Vec<u8>,
}

impl Texture {
    pub fn extension(&self) -> &'static str {
        match self.mime.as_str() {
            "image/jpeg" => "jpg",
            "image/webp" => "webp",
            "image/ktx2" => "ktx2",
            _ => "png",
        }
    }
    /// Type MIME d'après les premiers octets.
    pub fn sniff(data: &[u8]) -> &'static str {
        match data {
            [0x89, b'P', b'N', b'G', ..] => "image/png",
            [0xff, 0xd8, ..] => "image/jpeg",
            [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
            [0xab, b'K', b'T', b'X', ..] => "image/ktx2",
            _ => "application/octet-stream",
        }
    }
}

/// Conversion sRGB 8 bits → linéaire.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}
pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

impl Mesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Plages par matériau (une seule, sans matériau, si le maillage n'en déclare pas).
    pub fn groups(&self) -> Vec<SubMesh> {
        if self.submeshes.is_empty() {
            vec![SubMesh { first: 0, count: self.indices.len() as u32, material: None }]
        } else {
            self.submeshes.clone()
        }
    }

    /// Ajoute `other` à la fin (sommets, indices décalés, plages).
    pub fn append(&mut self, other: Mesh) {
        let base = self.positions.len() as u32;
        let first = self.indices.len() as u32;
        let groups = other.groups();
        if !self.uvs.is_empty() || !other.uvs.is_empty() {
            self.uvs.resize(self.positions.len(), [0.0; 2]);
        }
        self.positions.extend(other.positions);
        self.normals.extend(other.normals);
        if !other.uvs.is_empty() {
            self.uvs.extend(other.uvs);
        } else if !self.uvs.is_empty() {
            self.uvs.resize(self.positions.len(), [0.0; 2]);
        }
        self.indices.extend(other.indices.into_iter().map(|i| i + base));
        if self.submeshes.is_empty() && first > 0 {
            self.submeshes.push(SubMesh { first: 0, count: first, material: None });
        }
        for g in groups {
            if g.count == 0 {
                continue;
            }
            match self.submeshes.last_mut() {
                Some(last) if last.material == g.material && last.first + last.count == first + g.first => {
                    last.count += g.count
                }
                _ => self.submeshes.push(SubMesh { first: first + g.first, count: g.count, material: g.material }),
            }
        }
        if self.submeshes.iter().all(|g| g.material.is_none()) {
            self.submeshes.clear();
        }
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
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
}

impl Model {
    /// Couleur d'affichage d'une pièce : couleur de base de son premier matériau (sRGB 8 bits).
    pub fn display_color(&self, part: usize) -> Option<[u8; 4]> {
        let m = self.parts.get(part)?.mesh.submeshes.iter().find_map(|g| g.material)?;
        let c = self.materials.get(m as usize)?.base_color;
        let q = |v: f32| (linear_to_srgb(v) * 255.0).round() as u8;
        Some([q(c[0]), q(c[1]), q(c[2]), 255])
    }

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
