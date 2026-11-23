//! Arbre d'assemblage à l'export (D-35) : sous-ensemble, deux instances d'un même prototype (l'une tournée et en
//! miroir), écrites en GLB, FBX et OBJ puis relues : positions globales retrouvées, géométrie partagée.

use obv_io::{
    Mesh, Model, Node, Part, Placement, SubMesh,
    scene::{self, from_rows_3x4},
};
use obv_mesh::{Compression, ConvertOptions, Format, Layout};

/// Coin de cube (3 faces) dont l'origine est le coin : une géométrie asymétrique, pour voir les erreurs de repère.
fn proto() -> Mesh {
    let p = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [0.0, 0.0, 10.0]];
    Mesh {
        positions: p.to_vec(),
        normals: vec![[0.0, 0.0, 1.0]; 4],
        uvs: Vec::new(),
        indices: vec![0, 1, 2, 0, 2, 3, 0, 3, 1],
        submeshes: vec![SubMesh { first: 0, count: 9, material: None }],
    }
}

fn place(mesh: &Mesh, w: &scene::Mat4) -> Mesh {
    let mut m = mesh.clone();
    for p in &mut m.positions {
        *p = scene::point(w, p.map(f64::from)).map(|v| v as f32);
    }
    if scene::det3(w) < 0.0 {
        for t in m.indices.chunks_exact_mut(3) {
            t.swap(1, 2);
        }
    }
    m
}

fn model() -> Model {
    let t = |x: f64, y: f64, z: f64| from_rows_3x4(&[1.0, 0.0, 0.0, x, 0.0, 1.0, 0.0, y, 0.0, 0.0, 1.0, z]);
    // Rotation de 90° autour de Y, miroir en X, translation.
    let rot_mirror = from_rows_3x4(&[0.0, 0.0, -1.0, 40.0, 0.0, 1.0, 0.0, 5.0, -1.0, 0.0, 0.0, 0.0]);
    let nodes = vec![
        Node { name: "Ensemble".into(), parent: None, local: t(100.0, 0.0, 0.0) },
        Node { name: "Couvercle".into(), parent: Some(0), local: t(0.0, 20.0, 0.0) },
        Node { name: "Vis A".into(), parent: Some(1), local: t(5.0, 0.0, 0.0) },
        Node { name: "Vis B".into(), parent: Some(1), local: rot_mirror },
    ];
    let w = scene::world_matrices(&nodes);
    let parts = vec![
        Part { name: "Vis A".into(), mesh: place(&proto(), &w[2]) },
        Part { name: "Vis B".into(), mesh: place(&proto(), &w[3]) },
    ];
    Model {
        parts,
        unit_m: 0.001,
        nodes,
        placements: vec![Placement { node: 2, proto: 0 }, Placement { node: 3, proto: 0 }],
        ..Default::default()
    }
}

/// Même ensemble de points (à `eps` près, unités du modèle), ordre indifférent.
fn same_points(a: &Model, b: &Model, scale_b: f64, eps: f64) {
    let pts = |m: &Model, s: f64| {
        let mut v: Vec<[i64; 3]> = m
            .parts
            .iter()
            .flat_map(|p| p.mesh.positions.iter().map(move |q| q.map(|c| ((c as f64 * s) / eps).round() as i64)))
            .collect();
        v.sort();
        v.dedup();
        v
    };
    assert_eq!(pts(a, 1.0), pts(b, scale_b));
}

fn convert(model: &Model, format: Format, compression: Compression, name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("obv-mesh-assemblage");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let opts =
        ConvertOptions { format, tolerance_m: 0.0, keep_silhouette: true, compression, layout: Layout::Assembly };
    obv_mesh::convert(model, &path, opts, &|_| {}).unwrap();
    path
}

#[test]
fn glb_arbre_et_geometrie_partagee() {
    let m = model();
    let mut modes = vec![Compression::None, Compression::Quantize, Compression::Meshopt];
    if obv_draco::available() {
        modes.push(Compression::Draco);
    }
    for c in modes {
        let path = convert(&m, Format::Glb, c, "assemblage.glb");
        let back = obv_io::read_glb(&path).unwrap();
        // Mètres → millimètres.
        same_points(&m, &back, 1000.0, 1e-3);
        let bytes = std::fs::read(&path).unwrap();
        let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let json: serde_json::Value = serde_json::from_slice(&bytes[20..20 + len]).unwrap();
        assert_eq!(json["meshes"].as_array().unwrap().len(), 1, "{c:?} : un seul maillage partagé");
        let names: Vec<&str> = json["nodes"].as_array().unwrap().iter().filter_map(|n| n["name"].as_str()).collect();
        for n in ["Ensemble", "Couvercle", "Vis A", "Vis B"] {
            assert!(names.contains(&n), "{c:?} : nœud {n} absent de {names:?}");
        }
        assert_eq!(json["scenes"][0]["nodes"].as_array().unwrap().len(), 1, "{c:?} : une racine");
        // Repère de « Vis A » : translation propre (5 mm), pas celle de la boîte englobante.
        let vis_a = json["nodes"].as_array().unwrap().iter().find(|n| n["name"] == "Vis A").unwrap();
        let mtx = vis_a["matrix"].as_array().unwrap();
        assert!((mtx[12].as_f64().unwrap() - 0.005).abs() < 1e-9, "{c:?} : {mtx:?}");
        // Relu, chaque instance garde son nœud et le prototype commun.
        assert!(back.has_structure());
        assert_eq!(back.placements[0].proto, back.placements[1].proto);
    }
}

#[test]
fn fbx_arbre_et_positions() {
    let m = model();
    let path = convert(&m, Format::Fbx, Compression::None, "assemblage.fbx");
    let back = obv_io::read_fbx(&path).unwrap();
    // ufbx ramène en mètres.
    same_points(&m, &back, 1000.0, 1e-3);
}

#[test]
fn obj_instances_globales_nommees_par_chemin() {
    let m = model();
    let path = convert(&m, Format::Obj, Compression::None, "assemblage.obj");
    let back = obv_io::read_obj(&path).unwrap();
    same_points(&m, &back, 1.0, 1e-3);
    let names: Vec<&str> = back.parts.iter().map(|p| p.name.as_str()).collect();
    assert!(names.iter().any(|n| n.contains("Couvercle") && n.ends_with("Vis B")), "{names:?}");
}

#[test]
fn a_plat_et_selection() {
    let m = model();
    let dir = std::env::temp_dir().join("obv-mesh-assemblage");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plat.glb");
    let opts = ConvertOptions {
        format: Format::Glb,
        tolerance_m: 0.0,
        keep_silhouette: true,
        compression: Compression::None,
        layout: Layout::Flat,
    };
    let r = obv_mesh::convert_parts(&m, Some(&[false, true]), &path, opts, &|_| {}).unwrap();
    assert_eq!(r.parts.len(), 1);
    let back = obv_io::read_glb(&path).unwrap();
    assert_eq!(back.parts.len(), 1);
    let b = back.parts[0].mesh.bounds().unwrap();
    let a = m.parts[1].mesh.bounds().unwrap();
    for k in 0..3 {
        assert!((b.0[k] as f64 * 1000.0 - a.0[k] as f64).abs() < 1e-3, "{a:?} {b:?}");
    }
}
