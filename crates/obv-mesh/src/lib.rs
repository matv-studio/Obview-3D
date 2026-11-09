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
pub use optimize::{MeshReport, hausdorff, optimize_mesh};
use rayon::prelude::*;
pub use write::{Format, encode};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConvertOptions {
    pub format: Format,
    /// Écart maximal accepté en mètres ; 0 : géométrie gardée, seulement soudée et réordonnée.
    pub tolerance_m: f64,
}

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
pub fn optimize_model(model: &Model, tolerance_m: f64, progress: &(dyn Fn(f32) + Sync)) -> (Model, Report) {
    let done = AtomicUsize::new(0);
    let total: usize = model.parts.iter().map(|p| p.mesh.triangle_count()).sum::<usize>().max(1);
    let tol = (tolerance_m / model.unit_m) as f32;
    let results: Vec<(Part, MeshReport)> = model
        .parts
        .par_iter()
        .map(|p| {
            let (mesh, r) = optimize_mesh(&p.mesh, tol);
            let d = done.fetch_add(p.mesh.triangle_count(), Ordering::Relaxed) + p.mesh.triangle_count();
            progress(d as f32 / total as f32);
            (Part { name: p.name.clone(), mesh }, r)
        })
        .collect();
    let (parts, reports): (Vec<Part>, Vec<MeshReport>) = results.into_iter().unzip();
    (Model { parts, unit_m: model.unit_m }, Report { parts: reports, bytes: 0 })
}

/// Optimise puis écrit `path` (écriture dans un fichier temporaire puis renommage).
pub fn convert(
    model: &Model,
    path: &Path,
    opts: ConvertOptions,
    progress: &(dyn Fn(f32) + Sync),
) -> std::io::Result<Report> {
    let (out, mut report) = optimize_model(model, opts.tolerance_m, &|f| progress(f * 0.9));
    let bytes = encode(&out, opts.format);
    report.bytes = bytes.len();
    let tmp = path.with_extension(format!("{}.tmp", opts.format.extension()));
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, path)?;
    progress(1.0);
    Ok(report)
}
