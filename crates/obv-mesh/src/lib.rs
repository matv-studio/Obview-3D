//! Moteur de conversion d'Obview (D-07) : maillages optimisés sous un écart garanti, écrits en GLB, OBJ ou FBX.
//!
//! - `optimize` : soudure, simplification avec arêtes vives préservées, écart de Hausdorff mesuré et garanti,
//!   réordonnancement pour le cache des sommets, le recouvrement et la lecture mémoire ;
//! - `bvh` : arbre de boîtes pour les distances point-surface ;
//! - `write` : écrivains GLB, OBJ, FBX binaire 7.4.

pub mod bvh;
pub mod optimize;
pub mod visibility;
pub mod write;

use std::{
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

use obv_io::{
    Mesh, Model, Node, Part,
    scene::{self, Mat4},
};
pub use optimize::{MeshReport, SimplifyParams, hausdorff, optimize_mesh, optimize_mesh_with};
use rayon::prelude::*;
pub use write::{Compression, Format, OutputFile, WriteOptions, encode};

/// Organisation du fichier écrit (D-35).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    /// Arbre d'assemblage du fichier source : sous-ensembles, repères propres, géométries partagées.
    #[default]
    Assembly,
    /// Pièces à plat, positions globales.
    Flat,
}

/// Arbre du fichier écrit : nœuds (parents avant enfants) et instances des prototypes.
#[derive(Clone, Debug, Default)]
pub struct Structure {
    pub nodes: Vec<Node>,
    pub instances: Vec<Instance>,
}

/// Un prototype (pièce du modèle écrit, géométrie locale) posé sur un nœud.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Instance {
    pub node: u32,
    pub part: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConvertOptions {
    pub format: Format,
    /// Écart maximal accepté en mètres ; 0 : géométrie gardée, seulement soudée et réordonnée.
    pub tolerance_m: f64,
    /// Silhouette préservée : sommets des arêtes vives prioritaires.
    pub keep_silhouette: bool,
    /// GLB : compression des géométries.
    pub compression: Compression,
    pub layout: Layout,
}

impl ConvertOptions {
    /// Écart en mètres pour une position du curseur « fidélité ↔ performance » (0 : exact, 1 : le plus léger),
    /// relatif à la diagonale du modèle `diag_m` : de 3·10⁻⁵ à 10⁻² de la diagonale, en échelle logarithmique.
    pub fn tolerance_for_density(density: f64, diag_m: f64) -> f64 {
        if density <= 0.0 {
            return 0.0;
        }
        diag_m * 10f64.powf(-4.5 + 2.5 * density.clamp(0.0, 1.0))
    }

    /// Position du curseur pour un écart donné (inverse de [`Self::tolerance_for_density`]).
    pub fn density_for_tolerance(tolerance_m: f64, diag_m: f64) -> f64 {
        if tolerance_m <= 0.0 || diag_m <= 0.0 {
            return 0.0;
        }
        (((tolerance_m / diag_m).log10() + 4.5) / 2.5).clamp(0.0, 1.0)
    }
}

/// Position par défaut du curseur : optimisé, silhouette gardée (≈ 4·10⁻⁴ de la diagonale).
pub const DEFAULT_DENSITY: f64 = 0.45;

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub parts: Vec<MeshReport>,
    pub bytes: usize,
}

impl Report {
    pub fn triangles_in(&self) -> usize {
        self.parts.iter().map(|p| p.triangles_in).sum()
    }
    pub fn triangles_out(&self) -> usize {
        self.parts.iter().map(|p| p.triangles_out).sum()
    }
    /// Écart maximal mesuré, dans les unités du modèle.
    pub fn deviation(&self) -> f32 {
        self.parts.iter().map(|p| p.deviation).fold(0.0, f32::max)
    }
}

/// Optimise toutes les pièces en parallèle. `progress` reçoit la part accomplie (0 à 1).
pub fn optimize_model(
    model: &Model,
    tolerance_m: f64,
    keep_silhouette: bool,
    progress: &(dyn Fn(f32) + Sync),
) -> (Model, Report) {
    let done = AtomicUsize::new(0);
    let total: usize = model.parts.iter().map(|p| p.mesh.triangle_count()).sum::<usize>().max(1);
    let tol = (tolerance_m / model.unit_m) as f32;
    let results: Vec<(Part, MeshReport)> = model
        .parts
        .par_iter()
        .map(|p| {
            let (mesh, r) = optimize_mesh_with(&p.mesh, SimplifyParams { tolerance: tol, keep_silhouette });
            let d = done.fetch_add(p.mesh.triangle_count(), Ordering::Relaxed) + p.mesh.triangle_count();
            progress(d as f32 / total as f32);
            (Part { name: p.name.clone(), mesh }, r)
        })
        .collect();
    let (parts, reports): (Vec<Part>, Vec<MeshReport>) = results.into_iter().unzip();
    (
        Model {
            parts,
            unit_m: model.unit_m,
            materials: model.materials.clone(),
            textures: model.textures.clone(),
            ..Default::default()
        },
        Report { parts: reports, bytes: 0 },
    )
}

/// Géométrie d'une pièce ramenée dans le repère de son nœud (`world` : transformation globale du nœud).
fn to_local(mesh: &Mesh, world: &Mat4) -> Option<Mesh> {
    let inv = scene::inverse_affine(world)?;
    let mut m = mesh.clone();
    for p in &mut m.positions {
        *p = scene::point(&inv, p.map(f64::from)).map(|v| v as f32);
    }
    // Normales : transposée de l'inverse de `inv`, c'est-à-dire transposée de la partie linéaire de `world`.
    let wt = |v: [f64; 3]| -> [f64; 3] { std::array::from_fn(|c| (0..3).map(|r| world[c * 4 + r] * v[r]).sum()) };
    for n in &mut m.normals {
        let v = wt(n.map(f64::from));
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-30);
        *n = v.map(|c| (c / l) as f32);
    }
    // Instance en miroir : ses triangles ont été retournés à la lecture, on retrouve ceux du prototype.
    if scene::det3(world) < 0.0 {
        for t in m.indices.chunks_exact_mut(3) {
            t.swap(1, 2);
        }
    }
    Some(m)
}

/// Optimise les pièces gardées (`keep`, toutes si absent) selon l'organisation demandée : à plat, ou par
/// prototype (géométrie locale simplifiée une fois, posée sur chaque instance), avec l'arbre réduit aux nœuds
/// utiles. Le bilan compte chaque instance.
pub fn optimize_with_layout(
    model: &Model,
    keep: Option<&[bool]>,
    layout: Layout,
    tolerance_m: f64,
    keep_silhouette: bool,
    progress: &(dyn Fn(f32) + Sync),
) -> (Model, Option<Structure>, Report) {
    let kept: Vec<usize> =
        (0..model.parts.len()).filter(|i| keep.is_none_or(|k| k.get(*i).copied().unwrap_or(false))).collect();
    let flat = || {
        let sub = Model {
            parts: kept.iter().map(|&i| model.parts[i].clone()).collect(),
            unit_m: model.unit_m,
            materials: model.materials.clone(),
            textures: model.textures.clone(),
            ..Default::default()
        };
        let (m, r) = optimize_model(&sub, tolerance_m, keep_silhouette, progress);
        (m, None, r)
    };
    if layout == Layout::Flat || !model.has_structure() {
        return flat();
    }
    let world = scene::world_matrices(&model.nodes);
    // Prototypes des pièces gardées, dans l'ordre de première rencontre ; plus grande échelle de leurs instances.
    let mut proto_of: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    let mut protos: Vec<(usize, f64)> = Vec::new(); // (pièce source, échelle maximale)
    let mut inst = Vec::with_capacity(kept.len());
    for &i in &kept {
        let pl = model.placements[i];
        let w = &world[pl.node as usize];
        let k = *proto_of.entry(pl.proto).or_insert_with(|| {
            protos.push((i, 0.0));
            protos.len() - 1
        });
        protos[k].1 = protos[k].1.max(scene::max_scale(w));
        inst.push((pl.node, k));
    }
    let mut locals = Vec::with_capacity(protos.len());
    for &(i, _) in &protos {
        match to_local(&model.parts[i].mesh, &world[model.placements[i].node as usize]) {
            Some(m) => locals.push(Part { name: model.parts[i].name.clone(), mesh: m }),
            None => return flat(),
        }
    }
    // Écart : celui des instances, ramené à l'échelle du prototype.
    let done = AtomicUsize::new(0);
    let total: usize = locals.iter().map(|p| p.mesh.triangle_count()).sum::<usize>().max(1);
    let tol = tolerance_m / model.unit_m;
    let results: Vec<(Part, MeshReport)> = locals
        .par_iter()
        .zip(protos.par_iter())
        .map(|(p, &(_, s))| {
            let t = (tol / s.max(1e-12)) as f32;
            let (mesh, mut r) = optimize_mesh_with(&p.mesh, SimplifyParams { tolerance: t, keep_silhouette });
            r.deviation *= s as f32;
            let d = done.fetch_add(p.mesh.triangle_count(), Ordering::Relaxed) + p.mesh.triangle_count();
            progress(d as f32 / total as f32);
            (Part { name: p.name.clone(), mesh }, r)
        })
        .collect();
    // Arbre réduit aux ancêtres des instances, indices renumérotés (l'ordre parents-enfants est gardé).
    let mut used = vec![false; model.nodes.len()];
    for &(n, _) in &inst {
        let mut c = Some(n);
        while let Some(i) = c {
            if std::mem::replace(&mut used[i as usize], true) {
                break;
            }
            c = model.nodes[i as usize].parent;
        }
    }
    let mut remap = vec![u32::MAX; model.nodes.len()];
    let mut nodes = Vec::new();
    for (i, n) in model.nodes.iter().enumerate() {
        if used[i] {
            remap[i] = nodes.len() as u32;
            nodes.push(Node { parent: n.parent.map(|p| remap[p as usize]), ..n.clone() });
        }
    }
    let instances: Vec<Instance> =
        inst.iter().map(|&(n, k)| Instance { node: remap[n as usize], part: k as u32 }).collect();
    let reports: Vec<MeshReport> = inst.iter().map(|&(_, k)| results[k].1).collect();
    let parts: Vec<Part> = results.into_iter().map(|(p, _)| p).collect();
    (
        Model {
            parts,
            unit_m: model.unit_m,
            materials: model.materials.clone(),
            textures: model.textures.clone(),
            ..Default::default()
        },
        Some(Structure { nodes, instances }),
        Report { parts: reports, bytes: 0 },
    )
}

/// Optimise puis écrit `path` (écriture dans un fichier temporaire puis renommage).
pub fn convert(
    model: &Model,
    path: &Path,
    opts: ConvertOptions,
    progress: &(dyn Fn(f32) + Sync),
) -> std::io::Result<Report> {
    convert_parts(model, None, path, opts, progress)
}

/// Filtre des pièces exportées (D-38).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PartFilter {
    /// Pièces dont la diagonale de la boîte englobante est plus petite (mètres) : retirées.
    pub min_size_m: f64,
    /// Pièces invisibles de l'extérieur, quelle que soit la direction : retirées.
    pub remove_hidden: bool,
}

/// Diagonale de la boîte englobante de chaque pièce, en mètres.
pub fn part_sizes_m(model: &Model) -> Vec<f64> {
    model
        .parts
        .iter()
        .map(|p| {
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in &p.mesh.positions {
                for a in 0..3 {
                    lo[a] = lo[a].min(v[a]);
                    hi[a] = hi[a].max(v[a]);
                }
            }
            if p.mesh.positions.is_empty() {
                return 0.0;
            }
            (0..3).map(|a| ((hi[a] - lo[a]) as f64).powi(2)).sum::<f64>().sqrt() * model.unit_m
        })
        .collect()
}

/// Pièces gardées : `base` (une valeur par pièce, vide : toutes), moins les petites et les cachées.
pub fn select_parts(model: &Model, base: &[bool], filter: PartFilter, progress: &(dyn Fn(f32) + Sync)) -> Vec<bool> {
    let mut keep: Vec<bool> = (0..model.parts.len()).map(|i| base.get(i).copied().unwrap_or(true)).collect();
    if filter.min_size_m > 0.0 {
        for (k, size) in keep.iter_mut().zip(part_sizes_m(model)) {
            *k &= size >= filter.min_size_m;
        }
    }
    if filter.remove_hidden {
        keep = visibility::visible_parts(model, &keep, progress);
    }
    keep
}

/// Comme [`convert`], pour les pièces gardées par `keep` (une valeur par pièce) seulement.
pub fn convert_parts(
    model: &Model,
    keep: Option<&[bool]>,
    path: &Path,
    opts: ConvertOptions,
    progress: &(dyn Fn(f32) + Sync),
) -> std::io::Result<Report> {
    // Écart garanti partagé quand les positions seront quantifiées (GLB compressé) : 90 % à la simplification,
    // 10 % à la quantification ; leur somme reste dans l'écart demandé (D-30).
    let quantized = opts.format == Format::Glb && opts.compression != Compression::None;
    let share = if quantized { 0.9 } else { 1.0 };
    let (out, structure, mut report) =
        optimize_with_layout(model, keep, opts.layout, opts.tolerance_m * share, opts.keep_silhouette, &|f| {
            progress(f * 0.9)
        });
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "modele".into());
    let wopts = WriteOptions {
        format: opts.format,
        compression: opts.compression,
        tolerance: if quantized { (opts.tolerance_m * (1.0 - share) / model.unit_m) as f32 } else { 0.0 },
    };
    let files = write::encode_with(&out, structure.as_ref(), wopts, &stem);
    report.bytes = files.iter().map(|f| f.data.len()).sum();
    let dir = path.parent().unwrap_or(Path::new("."));
    for (i, f) in files.iter().enumerate() {
        // Le fichier principal prend exactement le chemin choisi ; les fichiers associés vont à côté.
        let target = if i == 0 { path.to_path_buf() } else { dir.join(&f.name) };
        // Fichier temporaire à côté, extension d'origine gardée (deux sorties de même nom n'entrent pas en collision).
        let mut tmp = target.clone().into_os_string();
        tmp.push(".obv-tmp");
        let tmp = std::path::PathBuf::from(tmp);
        std::fs::write(&tmp, &f.data)?;
        std::fs::rename(&tmp, &target)?;
    }
    progress(1.0);
    Ok(report)
}
