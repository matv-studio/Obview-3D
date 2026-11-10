//! Moteur de conversion d'Obview (D-07) : maillages optimisés sous un écart garanti, écrits en GLB, OBJ ou FBX.
//!
//! - `optimize` : soudure, simplification avec arêtes vives préservées, écart de Hausdorff mesuré et garanti,
//!   réordonnancement pour le cache des sommets, le recouvrement et la lecture mémoire ;
//! - `bvh` : arbre de boîtes pour les distances point-surface ;
//! - `write` : écrivains GLB, OBJ, FBX binaire 7.4.

pub mod bvh;
pub mod optimize;
pub mod write;

use std::{
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

use obv_io::{Model, Part};
pub use optimize::{MeshReport, SimplifyParams, hausdorff, optimize_mesh, optimize_mesh_with};
use rayon::prelude::*;
pub use write::{Compression, Format, OutputFile, WriteOptions, encode};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConvertOptions {
    pub format: Format,
    /// Écart maximal accepté en mètres ; 0 : géométrie gardée, seulement soudée et réordonnée.
    pub tolerance_m: f64,
    /// Silhouette préservée : sommets des arêtes vives prioritaires.
    pub keep_silhouette: bool,
    /// GLB : compression des géométries.
    pub compression: Compression,
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
        Model { parts, unit_m: model.unit_m, materials: model.materials.clone(), textures: model.textures.clone() },
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
    let (out, mut report) = optimize_model(model, opts.tolerance_m, opts.keep_silhouette, &|f| progress(f * 0.9));
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "modele".into());
    let wopts = WriteOptions {
        format: opts.format,
        compression: opts.compression,
        tolerance: (opts.tolerance_m / model.unit_m) as f32,
    };
    let files = encode(&out, wopts, &stem);
    report.bytes = files.iter().map(|f| f.data.len()).sum();
    let dir = path.parent().unwrap_or(Path::new("."));
    for (i, f) in files.iter().enumerate() {
        // Le fichier principal prend exactement le chemin choisi ; les fichiers associés vont à côté.
        let target = if i == 0 { path.to_path_buf() } else { dir.join(&f.name) };
        let tmp = target.with_extension("obv-tmp");
        std::fs::write(&tmp, &f.data)?;
        std::fs::rename(&tmp, &target)?;
    }
    progress(1.0);
    Ok(report)
}
