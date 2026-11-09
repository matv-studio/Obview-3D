//! Normales lissées par angle de pli : les sommets sont soudés par position, chaque coin reçoit la moyenne
//! (pondérée par l'aire et l'angle au sommet) des faces voisines dont l'écart avec sa face reste sous le seuil.
//! Un sommet n'est dupliqué que là où l'ombrage doit casser (arêtes vives).

use std::collections::HashMap;

use rayon::prelude::*;

use crate::Mesh;

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 0.0 { [a[0] / l, a[1] / l, a[2] / l] } else { [0.0, 0.0, 0.0] }
}

fn pos_key(p: V3) -> [u32; 3] {
    let k = |v: f32| if v == 0.0 { 0 } else { v.to_bits() };
    [k(p[0]), k(p[1]), k(p[2])]
}

/// Construit un maillage indexé à partir d'une soupe de triangles (`positions` par coin, 3 par triangle), avec des
/// normales lissées sous `crease_deg`.
pub fn from_triangle_soup(corners: &[V3], crease_deg: f32) -> Mesh {
    let mut ids: HashMap<[u32; 3], u32> = HashMap::with_capacity(corners.len() / 2);
    let mut unique: Vec<V3> = Vec::with_capacity(corners.len() / 2);
    let indices: Vec<u32> = corners
        .iter()
        .map(|p| {
            *ids.entry(pos_key(*p)).or_insert_with(|| {
                unique.push(*p);
                (unique.len() - 1) as u32
            })
        })
        .collect();
    smooth(&unique, &indices, crease_deg)
}

/// Normales lissées pour un maillage indexé dont les sommets sont déjà uniques par position.
pub fn smooth(positions: &[V3], indices: &[u32], crease_deg: f32) -> Mesh {
    let tri_count = indices.len() / 3;
    let cos_crease = crease_deg.to_radians().cos();
    // Normale unitaire de chaque face, et poids de chaque coin (angle au sommet × aire, à la Thürrner-Wüthrich
    // pondéré par l'aire : robuste aux triangles allongés des exports CAO).
    let faces: Vec<(V3, [f32; 3])> = (0..tri_count)
        .into_par_iter()
        .map(|t| {
            let v = [indices[3 * t] as usize, indices[3 * t + 1] as usize, indices[3 * t + 2] as usize];
            let p = [positions[v[0]], positions[v[1]], positions[v[2]]];
            let n = cross(sub(p[1], p[0]), sub(p[2], p[0]));
            let area = dot(n, n).sqrt();
            let angle = |a: V3, b: V3, c: V3| {
                let (e1, e2) = (norm(sub(b, a)), norm(sub(c, a)));
                dot(e1, e2).clamp(-1.0, 1.0).acos()
            };
            let w = [angle(p[0], p[1], p[2]), angle(p[1], p[2], p[0]), angle(p[2], p[0], p[1])];
            (norm(n), [w[0] * area, w[1] * area, w[2] * area])
        })
        .collect();
    // Faces incidentes à chaque sommet (tableau compact).
    let mut start = vec![0u32; positions.len() + 1];
    for &i in indices {
        start[i as usize + 1] += 1;
    }
    for i in 0..positions.len() {
        start[i + 1] += start[i];
    }
    let mut fill = start.clone();
    let mut incident = vec![0u32; indices.len()];
    for (c, &i) in indices.iter().enumerate() {
        incident[fill[i as usize] as usize] = c as u32;
        fill[i as usize] += 1;
    }
    // Normale de chaque coin.
    let corner_normals: Vec<V3> = (0..indices.len())
        .into_par_iter()
        .map(|c| {
            let v = indices[c] as usize;
            let nf = faces[c / 3].0;
            let mut acc = [0.0f32; 3];
            for &other in &incident[start[v] as usize..start[v + 1] as usize] {
                let (ng, w) = faces[other as usize / 3];
                if dot(nf, ng) >= cos_crease {
                    let k = w[other as usize % 3];
                    acc = [acc[0] + ng[0] * k, acc[1] + ng[1] * k, acc[2] + ng[2] * k];
                }
            }
            let n = norm(acc);
            if n == [0.0; 3] { nf } else { n }
        })
        .collect();
    // Sommets de sortie : un par (position, normale) distincts.
    let mut out = Mesh::default();
    let mut map: HashMap<(u32, [u32; 3]), u32> = HashMap::with_capacity(positions.len() * 2);
    out.indices = indices
        .iter()
        .zip(&corner_normals)
        .map(|(&v, n)| {
            *map.entry((v, pos_key(*n))).or_insert_with(|| {
                out.positions.push(positions[v as usize]);
                out.normals.push(*n);
                (out.positions.len() - 1) as u32
            })
        })
        .collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_a_aretes_vives() {
        // Soupe de 12 triangles : 8 positions, 24 sommets de sortie (3 normales par coin du cube).
        let c = |x: f32, y: f32, z: f32| [x, y, z];
        let p = [
            c(0., 0., 0.),
            c(1., 0., 0.),
            c(1., 1., 0.),
            c(0., 1., 0.),
            c(0., 0., 1.),
            c(1., 0., 1.),
            c(1., 1., 1.),
            c(0., 1., 1.),
        ];
        let quads = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [2, 3, 7, 6], [1, 2, 6, 5], [0, 4, 7, 3]];
        let mut soup = Vec::new();
        for q in quads {
            for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                soup.extend(t.iter().map(|&i| p[i]));
            }
        }
        let m = from_triangle_soup(&soup, 30.0);
        assert_eq!(m.triangle_count(), 12);
        assert_eq!(m.positions.len(), 24);
        // Lissé au-delà de 90° : 8 sommets.
        let m = from_triangle_soup(&soup, 100.0);
        assert_eq!(m.positions.len(), 8);
    }
}
