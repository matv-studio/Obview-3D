//! Lecture STEP et IGES par OpenCASCADE (D-27) : document XCAF (assemblages, noms, couleurs par face), surfaces
//! exactes maillées par BRepMesh à flèche et angle maîtrisés, une pièce par instance de l'assemblage.
//!
//! OpenCASCADE est lié statiquement, construit par `tools/occt.sh` (variable `OCCT_DIR` à la construction).
//! Sans elle, [`available`] est faux et [`read`] renvoie [`CadError::Unavailable`].

use std::path::Path;

use obv_io::{Model, Progress};

#[cfg(occt)]
mod ffi;

/// OpenCASCADE n'est pas sûr entre fils pour ce qui est partagé (application XCAF, messager, sessions
/// d'échange) : une seule lecture à la fois. Une lecture annulée rend la main au plus tôt (D-29).
#[cfg(occt)]
static OCCT: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    #[error("ouverture annulée")]
    Cancelled,
}

/// Réglages du maillage des surfaces exactes.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Flèche maximale en fraction de la diagonale du modèle.
    pub linear: f64,
    /// Écart angulaire maximal entre normales voisines (radians).
    pub angular: f64,
    /// Flèche absolue en mètres (nouveau maillage à l'export, D-35) ; remplace `linear` quand elle est donnée.
    pub absolute_m: Option<f64>,
}

impl Default for Options {
    /// Flèche de 1/4 000 de la diagonale et 0,35 rad (20°) : silhouettes lisses, budget de triangles proche
    /// d'un export de qualité « affichage fin » des modeleurs ; la conversion simplifie ensuite à écart garanti.
    fn default() -> Self {
        Options { linear: 2.5e-4, angular: 0.35, absolute_m: None }
    }
}

/// Lit `path` (STEP ou IGES d'après l'extension).
pub fn read(path: &Path, options: Options) -> Result<Model, CadError> {
    read_with(path, options, &Progress::default())
}

/// Comme [`read`], en publiant l'avancement dans `progress` ; s'arrête au plus tôt quand il est annulé.
pub fn read_with(path: &Path, options: Options, progress: &Progress) -> Result<Model, CadError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let iges = match ext.as_str() {
        "step" | "stp" => false,
        "iges" | "igs" => true,
        _ => return Err(CadError::Unsupported(ext)),
    };
    read_native(path, iges, options, progress)
}

#[cfg(not(occt))]
fn read_native(_: &Path, _: bool, _: Options, _: &Progress) -> Result<Model, CadError> {
    Err(CadError::Unavailable)
}

#[cfg(occt)]
fn read_native(path: &Path, iges: bool, options: Options, progress: &Progress) -> Result<Model, CadError> {
    use obv_io::{Material, Mesh, Part, Stage, SubMesh};
    let p = path.to_str().ok_or_else(|| CadError::Read("chemin non représentable en UTF-8".into()))?;
    let guard = OCCT.lock().unwrap_or_else(|e| e.into_inner());
    if progress.cancelled() {
        return Err(CadError::Cancelled);
    }
    let r = ffi::bridge::read_cad(
        p,
        iges,
        &ffi::bridge::CadOptions {
            linear: options.linear,
            angular: options.angular,
            absolute: options.absolute_m.unwrap_or(0.0),
        },
        &ffi::CadProgress(progress),
    );
    drop(guard);
    if progress.cancelled() {
        return Err(CadError::Cancelled);
    }
    progress.stage(Stage::Prepare);
    if !r.error.is_empty() {
        return Err(CadError::Read(r.error));
    }
    let mut model = Model { unit_m: r.unit_m, ..Default::default() };
    // Arbre d'assemblage (D-35). Les positions passent de Z vers le haut à Y vers le haut par `up` (rotation R) ;
    // chaque transformation locale L devient R·L·R⁻¹ : les transformations globales suivent les positions, et un
    // logiciel qui revient à Z vers le haut (Blender) retrouve exactement les repères de la CAO.
    let conj = |l: &[f64; 12]| -> [f64; 12] {
        // R : (x, y, z) → (x, z, -y) ; R·L·R⁻¹ par permutation des lignes et colonnes.
        let a = |r: usize, c: usize| l[r * 4 + c];
        let rr = [(0usize, 1.0), (2, 1.0), (1, -1.0)]; // ligne i de R·M = signe · ligne src de M
        let mut out = [0.0; 12];
        for (i, (si, sgi)) in rr.iter().enumerate() {
            for (j, (sj, sgj)) in rr.iter().enumerate() {
                out[i * 4 + j] = sgi * sgj * a(*si, *sj);
            }
            out[i * 4 + 3] = sgi * a(*si, 3);
        }
        out
    };
    model.nodes = r
        .nodes
        .iter()
        .map(|n| obv_io::Node {
            name: n.name.clone(),
            parent: (n.parent >= 0).then_some(n.parent as u32),
            local: obv_io::scene::from_rows_3x4(&conj(&n.local)),
        })
        .collect();
    // Prototype : forme de référence et matériaux (une même forme peut changer de couleur d'une instance à l'autre).
    let mut protos: Vec<(u32, Vec<Option<u32>>)> = Vec::new();
    let mut placements = Vec::with_capacity(r.parts.len());
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
        let key = (p.proto, mesh.submeshes.iter().map(|g| g.material).collect::<Vec<_>>());
        let proto = protos.iter().position(|k| *k == key).unwrap_or_else(|| {
            protos.push(key);
            protos.len() - 1
        });
        placements.push(obv_io::Placement { node: p.node, proto: proto as u32 });
        let name = if p.name.is_empty() { format!("Pièce {}", k + 1) } else { p.name };
        model.parts.push(Part { name, mesh });
    }
    model.placements = placements;
    Ok(model)
}

#[cfg(all(test, occt))]
mod tests {
    use super::*;

    /// Jeu d'essai écrit sous le verrou d'OpenCASCADE (essais en parallèle).
    fn write_test(path: &Path) -> bool {
        let _g = OCCT.lock().unwrap_or_else(|e| e.into_inner());
        ffi::bridge::write_test_step(path.to_str().unwrap())
    }

    #[test]
    fn assemblage_noms_couleurs() {
        let dir = std::env::temp_dir().join("obv-cad-essai");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("deux_cubes.step");
        assert!(write_test(&path), "écriture du jeu d'essai");
        let progress = Progress::default();
        let m = read_with(&path, Options::default(), &progress).unwrap();
        assert_eq!(progress.get().0, obv_io::Stage::Prepare);
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
        // Arbre (D-35) : l'assemblage puis ses deux instances ; la transformation globale de chaque instance,
        // appliquée à la géométrie de la première (ramenée dans son repère), redonne ses positions.
        assert!(m.has_structure());
        assert_eq!(m.nodes.iter().filter(|n| n.parent.is_none()).count(), 1, "{:?}", m.nodes);
        let world = obv_io::scene::world_matrices(&m.nodes);
        let (pa, pb) = (m.placements[0], m.placements[1]);
        assert_ne!(pa.node, pb.node);
        assert_eq!(obv_io::scene::path(&m.nodes, pb.node).split(" / ").last(), Some("Cube droit"));
        let inv_a = obv_io::scene::inverse_affine(&world[pa.node as usize]).unwrap();
        for (pa_p, pb_p) in m.parts[0].mesh.positions.iter().zip(&m.parts[1].mesh.positions) {
            let local = obv_io::scene::point(&inv_a, pa_p.map(f64::from));
            let back = obv_io::scene::point(&world[pb.node as usize], local);
            for k in 0..3 {
                assert!((back[k] - pb_p[k] as f64).abs() < 1e-4, "{back:?} {pb_p:?}");
            }
        }
        // Couleur différente : prototypes distincts malgré la même forme.
        assert_ne!(pa.proto, pb.proto);
    }

    #[test]
    fn fleche_absolue_et_angle() {
        let dir = std::env::temp_dir().join("obv-cad-essai-fleche");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("deux_cubes.step");
        assert!(write_test(&path));
        // Cube : maillage indépendant des réglages (12 triangles) ; on vérifie que les réglages passent.
        let opts = Options { angular: 2f64.to_radians(), absolute_m: Some(1e-6), ..Options::default() };
        let m = read_with(&path, opts, &Progress::default()).unwrap();
        assert_eq!(m.parts[0].mesh.triangle_count(), 12);
    }

    #[test]
    fn annulation() {
        let dir = std::env::temp_dir().join("obv-cad-essai-annule");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("deux_cubes.step");
        assert!(write_test(&path));
        let progress = Progress::default();
        progress.cancel();
        assert!(matches!(read_with(&path, Options::default(), &progress), Err(CadError::Cancelled)));
    }
}
