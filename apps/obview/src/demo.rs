//! Pièce de démonstration générée (essais et captures du dépôt : aucun fichier client dans git).

use obv_io::{Mesh, Model, Part};

fn push_quad(m: &mut Mesh, p: [[f32; 3]; 4], n: [f32; 3]) {
    let base = m.positions.len() as u32;
    m.positions.extend_from_slice(&p);
    m.normals.extend_from_slice(&[n; 4]);
    m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// Pavé à faces plates.
pub fn cuboid(c: [f32; 3], h: [f32; 3]) -> Mesh {
    let mut m = Mesh::default();
    let p = |sx: f32, sy: f32, sz: f32| [c[0] + sx * h[0], c[1] + sy * h[1], c[2] + sz * h[2]];
    push_quad(&mut m, [p(1., -1., 1.), p(1., -1., -1.), p(1., 1., -1.), p(1., 1., 1.)], [1., 0., 0.]);
    push_quad(&mut m, [p(-1., -1., -1.), p(-1., -1., 1.), p(-1., 1., 1.), p(-1., 1., -1.)], [-1., 0., 0.]);
    push_quad(&mut m, [p(-1., 1., 1.), p(1., 1., 1.), p(1., 1., -1.), p(-1., 1., -1.)], [0., 1., 0.]);
    push_quad(&mut m, [p(-1., -1., -1.), p(1., -1., -1.), p(1., -1., 1.), p(-1., -1., 1.)], [0., -1., 0.]);
    push_quad(&mut m, [p(-1., -1., 1.), p(1., -1., 1.), p(1., 1., 1.), p(-1., 1., 1.)], [0., 0., 1.]);
    push_quad(&mut m, [p(1., -1., -1.), p(-1., -1., -1.), p(-1., 1., -1.), p(1., 1., -1.)], [0., 0., -1.]);
    m
}

/// Cylindre d'axe Y, bouts plats, normales lisses sur le côté.
pub fn cylinder(c: [f32; 3], r: f32, h: f32, seg: u32) -> Mesh {
    let mut m = Mesh::default();
    let (y0, y1) = (c[1] - h * 0.5, c[1] + h * 0.5);
    for i in 0..seg {
        let a0 = i as f32 / seg as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / seg as f32 * std::f32::consts::TAU;
        let (s0, c0, s1, c1) = (a0.sin(), a0.cos(), a1.sin(), a1.cos());
        let base = m.positions.len() as u32;
        m.positions.extend([
            [c[0] + r * c0, y0, c[2] + r * s0],
            [c[0] + r * c1, y0, c[2] + r * s1],
            [c[0] + r * c1, y1, c[2] + r * s1],
            [c[0] + r * c0, y1, c[2] + r * s0],
        ]);
        m.normals.extend([[c0, 0., s0], [c1, 0., s1], [c1, 0., s1], [c0, 0., s0]]);
        m.indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
        for (y, ny, flip) in [(y1, 1.0, false), (y0, -1.0, true)] {
            let b = m.positions.len() as u32;
            m.positions.extend([[c[0], y, c[2]], [c[0] + r * c0, y, c[2] + r * s0], [c[0] + r * c1, y, c[2] + r * s1]]);
            m.normals.extend([[0., ny, 0.]; 3]);
            if flip { m.indices.extend([b, b + 1, b + 2]) } else { m.indices.extend([b, b + 2, b + 1]) }
        }
    }
    m
}

/// Support usiné : platine, moyeu, nervures, quatre vis.
pub fn bracket() -> Model {
    let mut parts = vec![
        Part { name: "Base plate".into(), mesh: cuboid([0.0, 0.0, 0.0], [2.0, 0.15, 1.3]) },
        Part { name: "Hub".into(), mesh: cylinder([0.0, 0.55, 0.0], 0.55, 0.95, 64) },
        Part { name: "Bore".into(), mesh: cylinder([0.0, 0.62, 0.0], 0.3, 1.0, 48) },
        Part { name: "Front rib".into(), mesh: cuboid([0.0, 0.4, 0.85], [0.08, 0.25, 0.32]) },
        Part { name: "Back rib".into(), mesh: cuboid([0.0, 0.4, -0.85], [0.08, 0.25, 0.32]) },
    ];
    for (i, (x, z)) in [(-1.6, -0.9), (1.6, -0.9), (-1.6, 0.9), (1.6, 0.9)].into_iter().enumerate() {
        parts.push(Part { name: format!("M6 screw · {}", i + 1), mesh: cylinder([x, 0.2, z], 0.16, 0.12, 32) });
    }
    Model { parts, unit_m: 0.01, ..Default::default() }
}
