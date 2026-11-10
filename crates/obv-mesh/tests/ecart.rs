//! Écart garanti de bout en bout (D-30) : simplification puis quantification des positions (GLB compressé),
//! relu par le lecteur d'Obview ; la distance de Hausdorff au modèle d'origine reste dans l'écart demandé.

use obv_io::{Mesh, Model, Part, SubMesh};
use obv_mesh::{Compression, ConvertOptions, Format, convert, hausdorff};

/// Sphère de 80 mm de rayon, fine (≈ 20 000 triangles), en millimètres.
fn sphere() -> Model {
    let (rings, segs) = (100u32, 100u32);
    let r = 80.0f32;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    for i in 0..=rings {
        let t = std::f32::consts::PI * i as f32 / rings as f32;
        for j in 0..=segs {
            let p = std::f32::consts::TAU * j as f32 / segs as f32;
            let n = [t.sin() * p.cos(), t.cos(), t.sin() * p.sin()];
            positions.push(n.map(|v| v * r));
            normals.push(n);
        }
    }
    let mut indices = Vec::new();
    for i in 0..rings {
        for j in 0..segs {
            let a = i * (segs + 1) + j;
            let b = a + segs + 1;
            indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    let count = indices.len() as u32;
    let mesh = Mesh {
        positions,
        normals,
        uvs: Vec::new(),
        indices,
        submeshes: vec![SubMesh { first: 0, count, material: None }],
    };
    Model {
        parts: vec![Part { name: "Sphère".into(), mesh }],
        unit_m: 0.001,
        materials: Vec::new(),
        textures: Vec::new(),
    }
}

#[test]
fn ecart_garanti_avec_quantification() {
    let model = sphere();
    let dir = std::env::temp_dir().join("obv-mesh-ecart");
    std::fs::create_dir_all(&dir).unwrap();
    // Écart de 0,05 mm : simplification forte, quantification active (pas de 16 bits : 2,4 µm).
    let tol_m = 0.05e-3;
    for compression in Compression::ALL.into_iter().filter(|c| c.available()) {
        let path = dir.join(format!("{compression:?}.glb"));
        let opts = ConvertOptions { format: Format::Glb, tolerance_m: tol_m, keep_silhouette: false, compression };
        convert(&model, &path, opts, &|_| {}).unwrap();
        let back = obv_io::read_glb(&path).unwrap();
        // Relu en mètres : ramené en millimètres.
        let mut mesh = back.parts[0].mesh.clone();
        let k = (back.unit_m / model.unit_m) as f32;
        for p in &mut mesh.positions {
            *p = p.map(|v| v * k);
        }
        let d = hausdorff(&model.parts[0].mesh, &mesh);
        assert!(d <= 0.05 * 1.0001, "{compression:?} : écart {d} mm > 0,05 mm");
        assert!(mesh.triangle_count() < model.parts[0].mesh.triangle_count(), "{compression:?} : rien de simplifié");
    }
}
