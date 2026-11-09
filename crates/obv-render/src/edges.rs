//! Arêtes vives : bords libres et arêtes entre faces dont l'angle dépasse un seuil. Les sommets sont soudés par
//! position exacte (les maillages exportés dupliquent les sommets le long des arêtes vives).

use std::collections::HashMap;

fn key(p: [f32; 3]) -> [u32; 3] {
    // -0.0 et 0.0 sont la même position.
    let k = |v: f32| if v == 0.0 { 0 } else { v.to_bits() };
    [k(p[0]), k(p[1]), k(p[2])]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Angle en dessous duquel deux faces voisines sont tenues pour coplanaires (jamais de contour entre elles).
const COPLANAR_DEG: f32 = 1.0;

/// Arêtes à tracer, par groupes de quatre points : extrémités, puis normales des deux faces voisines.
/// - Arête vive (bord libre, angle ≥ `angle_deg`, plus de deux faces) : normales nulles, toujours tracée.
/// - Arête douce (angle entre 1° et `angle_deg`) : tracée seulement quand elle est sur le contour apparent
///   (une face vers l'œil, l'autre non), test fait à chaque image par le nuanceur.
pub fn feature_edges(positions: &[[f32; 3]], indices: &[u32], angle_deg: f32) -> Vec<[f32; 3]> {
    let mut ids: HashMap<[u32; 3], u32> = HashMap::with_capacity(positions.len());
    let welded: Vec<u32> = positions
        .iter()
        .map(|p| {
            let n = ids.len() as u32;
            *ids.entry(key(*p)).or_insert(n)
        })
        .collect();
    let mut rep = vec![0u32; ids.len()];
    for (i, w) in welded.iter().enumerate() {
        rep[*w as usize] = i as u32;
    }
    let tri_count = indices.len() / 3;
    let mut normals = Vec::with_capacity(tri_count);
    // Arête (a < b) -> (première face, seconde face ou u32::MAX, nombre de faces).
    let mut edges: HashMap<(u32, u32), (u32, u32, u32)> = HashMap::with_capacity(tri_count * 3 / 2);
    for t in 0..tri_count {
        let v = [indices[3 * t], indices[3 * t + 1], indices[3 * t + 2]];
        let (a, b, c) = (positions[v[0] as usize], positions[v[1] as usize], positions[v[2] as usize]);
        let n = cross(sub(b, a), sub(c, a));
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        normals.push(if l > 0.0 { [n[0] / l, n[1] / l, n[2] / l] } else { [0.0; 3] });
        let w = [welded[v[0] as usize], welded[v[1] as usize], welded[v[2] as usize]];
        if w[0] == w[1] || w[1] == w[2] || w[0] == w[2] {
            continue; // triangle dégénéré
        }
        for k in 0..3 {
            let (x, y) = (w[k], w[(k + 1) % 3]);
            let e = edges.entry((x.min(y), x.max(y))).or_insert((t as u32, u32::MAX, 0));
            if e.2 == 1 {
                e.1 = t as u32;
            }
            e.2 += 1;
        }
    }
    let cos_limit = angle_deg.to_radians().cos();
    let cos_flat = COPLANAR_DEG.to_radians().cos();
    let mut out = Vec::new();
    for ((a, b), (f0, f1, count)) in edges {
        let normals_out = match count {
            2 => {
                let (n0, n1) = (normals[f0 as usize], normals[f1 as usize]);
                let c = n0[0] * n1[0] + n0[1] * n1[1] + n0[2] * n1[2];
                if c < cos_limit {
                    Some([[0.0; 3]; 2])
                } else if c < cos_flat {
                    Some([n0, n1])
                } else {
                    None
                }
            }
            _ => Some([[0.0; 3]; 2]),
        };
        if let Some([n0, n1]) = normals_out {
            out.push(positions[rep[a as usize] as usize]);
            out.push(positions[rep[b as usize] as usize]);
            out.push(n0);
            out.push(n1);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cube à sommets dupliqués par face (comme un export) : 12 arêtes vives, aucune diagonale.
    #[test]
    fn cube() {
        let mut p = Vec::new();
        let mut idx = Vec::new();
        let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
            ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
        ];
        for (n, u, v) in faces {
            let base = p.len() as u32;
            for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                p.push(std::array::from_fn(|i| n[i] + u[i] * su + v[i] * sv));
            }
            idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        let e = feature_edges(&p, &idx, 30.0);
        assert_eq!(e.len(), 12 * 4);
        assert!(e.chunks(4).all(|c| c[2] == [0.0; 3]), "arêtes du cube : toutes vives");
    }
}
