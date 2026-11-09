//! Optimisation d'un maillage : soudure, simplification sous un écart garanti, réordonnancement pour le GPU.
//!
//! La simplification (meshoptimizer, quadriques avec attributs) garde les discontinuités de normales : les
//! arêtes vives restent nettes. L'erreur annoncée par l'algorithme n'est qu'une estimation : l'écart réel est
//! mesuré dans les deux sens (Hausdorff, sommets et centres des triangles) et la simplification est reprise avec
//! une cible plus serrée tant qu'il dépasse la tolérance.

use meshopt::{SimplifyOptions, VertexDataAdapter};
use obv_io::Mesh;
use rayon::prelude::*;

use crate::bvh::Bvh;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeshReport {
    pub triangles_in: usize,
    pub triangles_out: usize,
    pub vertices_out: usize,
    /// Écart maximal mesuré entre la surface d'origine et le résultat (unités du maillage).
    pub deviation: f32,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Vertex {
    p: [f32; 3],
    n: [f32; 3],
}

/// Écart de Hausdorff approché par échantillons : sommets et centres des triangles de chaque surface, distance à
/// l'autre surface, maximum des deux sens.
pub fn hausdorff(a: &Mesh, b: &Mesh) -> f32 {
    let one_way = |from: &Mesh, to: &Bvh| -> f32 {
        let centers = from.indices.par_chunks_exact(3).map(|t| {
            let p = [from.positions[t[0] as usize], from.positions[t[1] as usize], from.positions[t[2] as usize]];
            std::array::from_fn(|i| (p[0][i] + p[1][i] + p[2][i]) / 3.0)
        });
        let verts = from.positions.par_iter().copied();
        verts.chain(centers).map(|p: [f32; 3]| to.distance(p)).reduce(|| 0.0, f32::max)
    };
    let (ba, bb) = rayon::join(|| Bvh::new(&a.positions, &a.indices), || Bvh::new(&b.positions, &b.indices));
    let (d1, d2) = rayon::join(|| one_way(a, &bb), || one_way(b, &ba));
    d1.max(d2)
}

/// Optimise `mesh` ; `tolerance` : écart maximal accepté (unités du maillage), 0 pour ne rien simplifier.
/// Rapproche les positions distantes de moins de `eps` (coutures à 1e-7 près des exports) : elles deviennent
/// identiques, et la simplification les traite comme une seule arête (sans fente entre les deux bords).
fn snap_positions(positions: &mut [[f32; 3]], eps: f32) {
    use std::collections::HashMap;
    if eps <= 0.0 {
        return;
    }
    let cell = |p: [f32; 3]| p.map(|v| (v / eps).floor() as i64);
    let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::with_capacity(positions.len());
    for i in 0..positions.len() {
        let p = positions[i];
        let c = cell(p);
        let mut found = None;
        'search: for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(list) = grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                        for &j in list {
                            let q = positions[j as usize];
                            let d = (0..3).map(|k| (p[k] - q[k]).powi(2)).sum::<f32>();
                            if d <= eps * eps {
                                found = Some(q);
                                break 'search;
                            }
                        }
                    }
                }
            }
        }
        match found {
            Some(q) => positions[i] = q,
            None => grid.entry(c).or_default().push(i as u32),
        }
    }
}

pub fn optimize_mesh(mesh: &Mesh, tolerance: f32) -> (Mesh, MeshReport) {
    let triangles_in = mesh.triangle_count();
    let mut positions = mesh.positions.clone();
    if let Some((lo, hi)) = mesh.bounds() {
        let diag = (0..3).map(|i| (hi[i] - lo[i]).powi(2)).sum::<f32>().sqrt();
        snap_positions(&mut positions, diag * 1e-6);
    }
    // Soudure exacte des sommets identiques (position et normale).
    let verts: Vec<Vertex> = positions.iter().zip(&mesh.normals).map(|(p, n)| Vertex { p: *p, n: *n }).collect();
    let (unique, remap) = meshopt::generate_vertex_remap(&verts, Some(&mesh.indices));
    let mut indices = meshopt::remap_index_buffer(Some(&mesh.indices), verts.len(), &remap);
    let mut verts = meshopt::remap_vertex_buffer(&verts, unique, &remap);

    let mut deviation = 0.0;
    if tolerance > 0.0 && !indices.is_empty() {
        let bytes: &[u8] = unsafe {
            // SAFETY: `Vertex` est `repr(C)` sans octets de remplissage (six f32).
            std::slice::from_raw_parts(verts.as_ptr().cast(), verts.len() * std::mem::size_of::<Vertex>())
        };
        let adapter = VertexDataAdapter::new(bytes, std::mem::size_of::<Vertex>(), 0).expect("sommets");
        let normals: Vec<f32> = verts.iter().flat_map(|v| v.n).collect();
        // Poids des normales, à l'échelle de la tolérance (erreur absolue) : un écart de normale d'environ 60° pèse
        // une demi-tolérance. Assez pour éviter les plis, sans freiner la réduction des surfaces lisses.
        let weights = [tolerance * 0.5; 3];
        let locks = vec![false; verts.len()];
        // Recherche de la cible : l'erreur estimée par l'algorithme (quadriques aux sommets) sous-estime l'écart
        // réel, surtout sur les surfaces courbes où restent des triangles allongés. Correction proportionnelle
        // tant qu'aucune borne n'est connue, puis dichotomie. Deux réglages sont essayés (triangles libres, et
        // triangles régularisés, meilleurs sur les surfaces courbes) ; le résultat accepté le plus léger l'emporte.
        let original =
            Mesh { positions: verts.iter().map(|v| v.p).collect(), normals: Vec::new(), indices: indices.clone() };
        let search = |options: SimplifyOptions| -> Option<(Vec<u32>, f32)> {
            let mut target = tolerance;
            let (mut lo, mut hi): (f32, Option<f32>) = (0.0, None);
            let mut best: Option<(Vec<u32>, f32)> = None;
            for _ in 0..8 {
                let candidate = meshopt::simplify_with_attributes_and_locks(
                    &indices, &adapter, &normals, &weights, 12, &locks, 0, target, options, None,
                );
                let simplified =
                    Mesh { positions: original.positions.clone(), normals: Vec::new(), indices: candidate.clone() };
                let d = hausdorff(&original, &simplified);
                if std::env::var_os("OBV_DEBUG_SIMPLIFY").is_some() {
                    eprintln!("{options:?} cible {target:.6} : {} triangles, écart {d:.6}", candidate.len() / 3);
                }
                if d <= tolerance {
                    if best.as_ref().is_none_or(|(b, _)| candidate.len() < b.len()) {
                        best = Some((candidate, d));
                    }
                    lo = target;
                    if d > tolerance * 0.85 {
                        break;
                    }
                    target = match hi {
                        Some(h) => (lo + h) * 0.5,
                        None => target * (tolerance / d.max(tolerance * 0.05)).min(2.0),
                    };
                } else {
                    hi = Some(target);
                    target = if lo > 0.0 { (lo + target) * 0.5 } else { target * tolerance / d * 0.9 };
                }
            }
            best
        };
        let base = SimplifyOptions::ErrorAbsolute | SimplifyOptions::PreserveFolds;
        let (a, b) = rayon::join(|| search(base), || search(base | SimplifyOptions::RegularizeLight));
        let best = match (a, b) {
            (Some(a), Some(b)) => Some(if b.0.len() < a.0.len() { b } else { a }),
            (a, b) => a.or(b),
        };
        if let Some((s, d)) = best {
            indices = s;
            deviation = d;
        }
    }
    // Ordre des triangles pour le cache des sommets puis le recouvrement, ordre des sommets pour la lecture.
    meshopt::optimize_vertex_cache_in_place(&mut indices, verts.len());
    {
        let bytes: &[u8] = unsafe {
            // SAFETY: voir plus haut.
            std::slice::from_raw_parts(verts.as_ptr().cast(), verts.len() * std::mem::size_of::<Vertex>())
        };
        let adapter = VertexDataAdapter::new(bytes, std::mem::size_of::<Vertex>(), 0).expect("sommets");
        meshopt::optimize_overdraw_in_place(&mut indices, &adapter, 1.05);
    }
    let kept = meshopt::optimize_vertex_fetch_in_place(&mut indices, &mut verts);
    verts.truncate(kept);
    let out =
        Mesh { positions: verts.iter().map(|v| v.p).collect(), normals: verts.iter().map(|v| v.n).collect(), indices };
    let report =
        MeshReport { triangles_in, triangles_out: out.triangle_count(), vertices_out: out.positions.len(), deviation };
    (out, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sphère UV très dense : la simplification réduit fortement et respecte la tolérance mesurée.
    fn sphere(n: usize) -> Mesh {
        let mut m = Mesh::default();
        for j in 0..=n {
            let t = std::f32::consts::PI * j as f32 / n as f32;
            for i in 0..=2 * n {
                let f = std::f32::consts::PI * i as f32 / n as f32;
                let p = [t.sin() * f.cos(), t.cos(), t.sin() * f.sin()];
                m.positions.push(p);
                m.normals.push(p);
            }
        }
        let w = 2 * n + 1;
        for j in 0..n {
            for i in 0..2 * n {
                let a = (j * w + i) as u32;
                let (b, c, d) = (a + 1, a + w as u32, a + w as u32 + 1);
                m.indices.extend([a, c, b, b, c, d]);
            }
        }
        m
    }

    #[test]
    fn tolerance_respectee() {
        let s = sphere(120);
        let (out, r) = optimize_mesh(&s, 0.002);
        assert!(r.triangles_out * 4 < r.triangles_in, "{r:?}");
        assert!(r.deviation <= 0.002, "{r:?}");
        // Mesure indépendante.
        assert!(hausdorff(&s, &out) <= 0.0021);
        // Sans tolérance : même nombre de triangles.
        let (_, r0) = optimize_mesh(&s, 0.0);
        assert_eq!(r0.triangles_out, r0.triangles_in);
    }
}
