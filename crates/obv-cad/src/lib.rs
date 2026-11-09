//! Lecture STEP et IGES par OpenCASCADE (D-27) : document XCAF (assemblages, noms, couleurs par face), surfaces
//! exactes maillées par BRepMesh à flèche et angle maîtrisés, une pièce par instance de l'assemblage.
//!
//! OpenCASCADE est lié statiquement, construit par `tools/occt.sh` (variable `OCCT_DIR` à la construction).
//! Sans elle, [`available`] est faux et [`read`] renvoie [`CadError::Unavailable`].

use std::path::Path;

use obv_io::Model;

#[cfg(occt)]
mod ffi;

/// Extensions lues par ce module.
pub const EXTENSIONS: &[&str] = &["step", "stp", "iges", "igs"];

/// OpenCASCADE est-il présent dans cette construction ?
pub const fn available() -> bool {
    cfg!(occt)
}

#[derive(Debug, thiserror::Error)]
pub enum CadError {
    #[error("lecture STEP / IGES absente de cette construction")]
    Unavailable,
    #[error("format non pris en charge : {0}")]
    Unsupported(String),
    #[error("{0}")]
    Read(String),
}

/// Réglages du maillage des surfaces exactes.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Flèche maximale en fraction de la diagonale du modèle.
    pub linear: f64,
    /// Écart angulaire maximal entre normales voisines (radians).
    pub angular: f64,
}

impl Default for Options {
    /// Flèche de 1/4 000 de la diagonale et 0,35 rad (20°) : silhouettes lisses, budget de triangles proche
    /// d'un export de qualité « affichage fin » des modeleurs ; la conversion simplifie ensuite à écart garanti.
    fn default() -> Self {
        Options { linear: 2.5e-4, angular: 0.35 }
    }
}

/// Lit `path` (STEP ou IGES d'après l'extension).
pub fn read(path: &Path, options: Options) -> Result<Model, CadError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let iges = match ext.as_str() {
        "step" | "stp" => false,
        "iges" | "igs" => true,
        _ => return Err(CadError::Unsupported(ext)),
    };
    read_native(path, iges, options)
}

#[cfg(not(occt))]
fn read_native(_: &Path, _: bool, _: Options) -> Result<Model, CadError> {
    Err(CadError::Unavailable)
}

#[cfg(occt)]
fn read_native(path: &Path, iges: bool, options: Options) -> Result<Model, CadError> {
    use obv_io::{Material, Mesh, Part, SubMesh};
    let p = path.to_str().ok_or_else(|| CadError::Read("chemin non représentable en UTF-8".into()))?;
    let r =
        ffi::bridge::read_cad(p, iges, &ffi::bridge::CadOptions { linear: options.linear, angular: options.angular });
    if !r.error.is_empty() {
        return Err(CadError::Read(r.error));
    }
    let mut model = Model { unit_m: r.unit_m, ..Default::default() };
    // Un matériau par couleur distincte.
    let mut colors: Vec<[u32; 4]> = Vec::new();
    for (k, p) in r.parts.into_iter().enumerate() {
        // Les modeleurs CAO ont Z vers le haut, Obview Y : rotation de -90° autour de X, (x, y, z) → (x, z, -y).
        let up = |c: &[f32]| [c[0], c[2], -c[1]];
        let mut mesh = Mesh {
            positions: p.positions.chunks_exact(3).map(up).collect(),
            normals: p.normals.chunks_exact(3).map(up).collect(),
            uvs: Vec::new(),
            indices: p.indices,
            submeshes: Vec::new(),
        };
        if p.groups.iter().any(|g| g.has_color) {
            for g in &p.groups {
                let material = g.has_color.then(|| {
                    let bits = g.color.map(f32::to_bits);
                    let i = colors.iter().position(|c| *c == bits).unwrap_or_else(|| {
                        colors.push(bits);
                        model.materials.push(Material {
                            name: format!("Couleur {}", colors.len()),
                            base_color: g.color,
                            metallic: 0.0,
                            roughness: 0.5,
                            blend: g.color[3] < 1.0,
                            ..Default::default()
                        });
                        colors.len() - 1
                    });
                    i as u32
                });
                mesh.submeshes.push(SubMesh { first: g.first * 3, count: g.count * 3, material });
            }
        }
        let name = if p.name.is_empty() { format!("Pièce {}", k + 1) } else { p.name };
        model.parts.push(Part { name, mesh });
    }
    Ok(model)
}

#[cfg(all(test, occt))]
mod tests {
    use super::*;

    #[test]
    fn assemblage_noms_couleurs() {
        let dir = std::env::temp_dir().join("obv-cad-essai");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("deux_cubes.step");
        assert!(ffi::bridge::write_test_step(path.to_str().unwrap()), "écriture du jeu d'essai");
        let m = read(&path, Options::default()).unwrap();
        assert!((m.unit_m - 0.001).abs() < 1e-12, "{}", m.unit_m);
        assert_eq!(m.parts.len(), 2, "{:?}", m.parts.iter().map(|p| &p.name).collect::<Vec<_>>());
        for p in &m.parts {
            assert_eq!(p.mesh.triangle_count(), 12, "{}", p.name);
        }
        // Z du fichier devenu Y : la hauteur du cube est en Y, sa profondeur en -Z.
        let (a, b) = (m.parts[0].mesh.bounds().unwrap(), m.parts[1].mesh.bounds().unwrap());
        assert!((a.1[1] - a.0[1] - 10.0).abs() < 1e-4 && a.1[2] <= 1e-4, "{a:?}");
        assert!((b.0[0] - a.0[0] - 20.0).abs() < 1e-4, "{a:?} {b:?}");
        let names: Vec<&str> = m.parts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Cube gauche", "Cube droit"]);
        assert_eq!(m.display_color(0), None);
        assert_eq!(m.display_color(1), Some([255, 0, 0, 255]));
    }
}
