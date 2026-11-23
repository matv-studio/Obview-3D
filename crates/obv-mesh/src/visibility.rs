//! Pièces cachées (D-38) : une pièce est gardée si elle se voit depuis au moins une des directions réparties sur la
//! sphère. Chaque direction est une vue orthographique rendue sur le processeur (tampon de profondeur et d'indices
//! de pièce) ; les pièces transparentes ne cachent rien mais sont elles-mêmes visibles. Les triangles plus petits
//! qu'un pixel comptent par leur centre : une pièce minuscule mais visible n'est pas retirée.

use std::sync::atomic::{AtomicUsize, Ordering};

use obv_io::Model;
use rayon::prelude::*;

/// Directions de vue (spirale de Fibonacci sur la sphère entière).
pub const DIRECTIONS: usize = 128;
/// Côté des vues, en pixels.
pub const RESOLUTION: usize = 1024;

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn normalize(a: V3) -> V3 {
    let l = dot(a, a).sqrt().max(1e-30);
    [a[0] / l, a[1] / l, a[2] / l]
}

/// `n` directions réparties uniformément.
pub fn directions(n: usize) -> Vec<V3> {
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    (0..n)
        .map(|i| {
            let z = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
            let r = (1.0 - z * z).max(0.0).sqrt();
            let t = golden * i as f64;
            [r * t.cos(), r * t.sin(), z]
        })
        .collect()
}

/// Triangle d'une pièce, en coordonnées du modèle ; `opaque` : il cache ce qui est derrière lui.
struct Tri {
    p: [V3; 3],
    part: u32,
    opaque: bool,
}

/// Pièces visibles parmi celles gardées par `keep` (les autres restent à `false`).
pub fn visible_parts(model: &Model, keep: &[bool], progress: &(dyn Fn(f32) + Sync)) -> Vec<bool> {
    visible_parts_with(model, keep, DIRECTIONS, RESOLUTION, progress)
}

pub fn visible_parts_with(
    model: &Model,
    keep: &[bool],
    n_dirs: usize,
    res: usize,
    progress: &(dyn Fn(f32) + Sync),
) -> Vec<bool> {
    let n_parts = model.parts.len();
    let kept = |i: usize| keep.get(i).copied().unwrap_or(true);
    let transparent = |m: Option<u32>| {
        m.and_then(|m| model.materials.get(m as usize)).is_some_and(|m| m.blend || m.base_color[3] < 0.98)
    };
    let mut tris = Vec::new();
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for (i, part) in model.parts.iter().enumerate().filter(|(i, _)| kept(*i)) {
        let mesh = &part.mesh;
        let groups: Vec<(u32, u32, Option<u32>)> = if mesh.submeshes.is_empty() {
            vec![(0, mesh.indices.len() as u32, None)]
        } else {
            mesh.submeshes.iter().map(|s| (s.first, s.count, s.material)).collect()
        };
        for (first, count, material) in groups {
            let opaque = !transparent(material);
            let end = ((first + count) as usize).min(mesh.indices.len());
            for t in mesh.indices[first as usize..end].chunks_exact(3) {
                let p = [0, 1, 2].map(|k| {
                    let v = mesh.positions[t[k] as usize];
                    [v[0] as f64, v[1] as f64, v[2] as f64]
                });
                for v in &p {
                    for a in 0..3 {
                        lo[a] = lo[a].min(v[a]);
                        hi[a] = hi[a].max(v[a]);
                    }
                }
                tris.push(Tri { p, part: i as u32, opaque });
            }
        }
    }
    let mut seen = vec![false; n_parts];
    if tris.is_empty() {
        return seen;
    }
    let center = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
    let radius = (dot(sub(hi, lo), sub(hi, lo)).sqrt() * 0.5).max(1e-12) * 1.001;
    let dirs = directions(n_dirs);
    let done = AtomicUsize::new(0);
    let found = dirs
        .par_iter()
        .map_init(
            || (vec![f32::NEG_INFINITY; res * res], vec![u32::MAX; res * res]),
            |(depth, ids), d| {
                depth.fill(f32::NEG_INFINITY);
                ids.fill(u32::MAX);
                let mut local = vec![false; n_parts];
                view(&tris, center, radius, *d, res, depth, ids, &mut local);
                let k = done.fetch_add(1, Ordering::Relaxed) + 1;
                progress(k as f32 / dirs.len() as f32);
                local
            },
        )
        .reduce(
            || vec![false; n_parts],
            |mut a, b| {
                a.iter_mut().zip(b).for_each(|(x, y)| *x |= y);
                a
            },
        );
    for (s, f) in seen.iter_mut().zip(found) {
        *s = f;
    }
    seen
}

/// Une vue depuis la direction `d` (observateur du côté de `+d`) : passe opaque, puis triangles transparents
/// testés contre la profondeur.
#[allow(clippy::too_many_arguments)]
fn view(
    tris: &[Tri],
    center: V3,
    radius: f64,
    d: V3,
    res: usize,
    depth: &mut [f32],
    ids: &mut [u32],
    seen: &mut [bool],
) {
    let a = if d[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
    let u = normalize(cross(d, a));
    let v = cross(d, u);
    let s = res as f64 * 0.5 / radius;
    let h = res as f64 * 0.5;
    let project = |p: V3| {
        let q = sub(p, center);
        [dot(q, u) * s + h, dot(q, v) * s + h, dot(q, d)]
    };
    for pass in [true, false] {
        for t in tris.iter().filter(|t| t.opaque == pass) {
            let p = t.p.map(project);
            raster(p, res, |i, z| {
                if z > depth[i] {
                    if pass {
                        depth[i] = z;
                        ids[i] = t.part;
                    } else {
                        seen[t.part as usize] = true;
                    }
                }
            });
        }
        if pass {
            for id in ids.iter().filter(|id| **id != u32::MAX) {
                seen[*id as usize] = true;
            }
        }
    }
}

/// Pixels couverts par le triangle projeté (centres des pixels) ; un triangle qui n'en couvre aucun compte par le
/// pixel de son centre.
fn raster(p: [V3; 3], res: usize, mut f: impl FnMut(usize, f32)) {
    let min_x = p.iter().map(|q| q[0]).fold(f64::MAX, f64::min);
    let max_x = p.iter().map(|q| q[0]).fold(f64::MIN, f64::max);
    let min_y = p.iter().map(|q| q[1]).fold(f64::MAX, f64::min);
    let max_y = p.iter().map(|q| q[1]).fold(f64::MIN, f64::max);
    let x0 = (min_x - 0.5).ceil().max(0.0) as i64;
    let x1 = ((max_x - 0.5).floor() as i64).min(res as i64 - 1);
    let y0 = (min_y - 0.5).ceil().max(0.0) as i64;
    let y1 = ((max_y - 0.5).floor() as i64).min(res as i64 - 1);
    let area = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
    let mut any = false;
    if area.abs() > 1e-12 && x0 <= x1 && y0 <= y1 {
        let inv = 1.0 / area;
        let edge = |a: V3, b: V3, x: f64, y: f64| (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]);
        for y in y0..=y1 {
            let py = y as f64 + 0.5;
            for x in x0..=x1 {
                let px = x as f64 + 0.5;
                let w0 = edge(p[1], p[2], px, py) * inv;
                let w1 = edge(p[2], p[0], px, py) * inv;
                let w2 = 1.0 - w0 - w1;
                if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                    any = true;
                    let z = w0 * p[0][2] + w1 * p[1][2] + w2 * p[2][2];
                    f(y as usize * res + x as usize, z as f32);
                }
            }
        }
    }
    if !any {
        let c = [(p[0][0] + p[1][0] + p[2][0]) / 3.0, (p[0][1] + p[1][1] + p[2][1]) / 3.0];
        let z = (p[0][2] + p[1][2] + p[2][2]) / 3.0;
        let (x, y) = (c[0].floor(), c[1].floor());
        if x >= 0.0 && y >= 0.0 && (x as usize) < res && (y as usize) < res {
            f(y as usize * res + x as usize, z as f32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use obv_io::{Material, Mesh, Part, SubMesh};

    /// Cube fermé de centre `c` et de demi-côté `h`.
    fn cube(c: [f32; 3], h: f32, material: Option<u32>) -> Part {
        let mut positions = Vec::new();
        for i in 0..8 {
            positions.push([
                c[0] + if i & 1 != 0 { h } else { -h },
                c[1] + if i & 2 != 0 { h } else { -h },
                c[2] + if i & 4 != 0 { h } else { -h },
            ]);
        }
        let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let indices: Vec<u32> = quads.iter().flat_map(|q| [q[0], q[1], q[2], q[0], q[2], q[3]]).collect();
        let n = indices.len() as u32;
        Part {
            name: "cube".into(),
            mesh: Mesh {
                normals: vec![[0.0, 0.0, 1.0]; 8],
                positions,
                uvs: Vec::new(),
                indices,
                submeshes: vec![SubMesh { first: 0, count: n, material }],
            },
        }
    }

    #[test]
    fn piece_enfermee_retiree() {
        // Boîte, pièce enfermée, pièce dehors, minuscule pièce posée sur la boîte.
        let model = Model {
            parts: vec![
                cube([0.0; 3], 1.0, None),
                cube([0.0; 3], 0.3, None),
                cube([3.0, 0.0, 0.0], 0.5, None),
                cube([0.0, 0.0, 1.0005], 0.0004, None),
            ],
            unit_m: 1.0,
            ..Default::default()
        };
        let v = visible_parts_with(&model, &[true; 4], 32, 256, &|_| {});
        assert_eq!(v, vec![true, false, true, true]);
        // Pièce non gardée : ni cachée ni comptée.
        let v = visible_parts_with(&model, &[false, true, true, true], 32, 256, &|_| {});
        assert_eq!(v, vec![false, true, true, true]);
    }

    #[test]
    fn verre_ne_cache_rien() {
        let glass = Material { base_color: [1.0, 1.0, 1.0, 0.3], blend: true, ..Default::default() };
        let model = Model {
            parts: vec![cube([0.0; 3], 1.0, Some(0)), cube([0.0; 3], 0.3, None)],
            unit_m: 1.0,
            materials: vec![glass],
            ..Default::default()
        };
        assert_eq!(visible_parts_with(&model, &[true; 2], 32, 256, &|_| {}), vec![true, true]);
    }
}
