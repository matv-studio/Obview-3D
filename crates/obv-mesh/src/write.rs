//! Écriture des maillages : GLB (glTF 2.0 binaire, mètres), OBJ (millimètres), FBX binaire 7.4 (millimètres,
//! facteur d'unité déclaré). Une pièce = un nœud nommé.

use std::io::Write;

use obv_io::Model;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Glb,
    Obj,
    Fbx,
}

impl Format {
    pub const ALL: [Format; 3] = [Format::Glb, Format::Obj, Format::Fbx];
    pub fn extension(self) -> &'static str {
        match self {
            Format::Glb => "glb",
            Format::Obj => "obj",
            Format::Fbx => "fbx",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Format::Glb => "GLB",
            Format::Obj => "OBJ",
            Format::Fbx => "FBX",
        }
    }
}

/// Encode le modèle dans le format demandé.
pub fn encode(model: &Model, format: Format) -> Vec<u8> {
    match format {
        Format::Glb => glb(model),
        Format::Obj => obj(model),
        Format::Fbx => fbx(model),
    }
}

// ---- GLB ----

fn pad4(v: &mut Vec<u8>, byte: u8) {
    while !v.len().is_multiple_of(4) {
        v.push(byte);
    }
}

fn glb(model: &Model) -> Vec<u8> {
    use serde_json::json;
    let scale = model.unit_m as f32;
    let mut bin: Vec<u8> = Vec::new();
    let (mut views, mut accessors, mut meshes, mut nodes) = (vec![], vec![], vec![], vec![]);
    let mut view = |bin: &mut Vec<u8>, data: &[u8], target: u32| -> usize {
        pad4(bin, 0);
        let offset = bin.len();
        bin.extend_from_slice(data);
        views.push(json!({"buffer": 0, "byteOffset": offset, "byteLength": data.len(), "target": target}));
        views.len() - 1
    };
    for part in &model.parts {
        let m = &part.mesh;
        if m.indices.is_empty() {
            continue;
        }
        let pos: Vec<f32> = m.positions.iter().flat_map(|p| p.map(|v| v * scale)).collect();
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in pos.chunks(3) {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        let nrm: Vec<f32> = m.normals.iter().flatten().copied().collect();
        let pv = view(&mut bin, &f32_bytes(&pos), 34962);
        let nv = view(&mut bin, &f32_bytes(&nrm), 34962);
        let small = m.positions.len() <= u16::MAX as usize;
        let idx_bytes: Vec<u8> = if small {
            m.indices.iter().flat_map(|i| (*i as u16).to_le_bytes()).collect()
        } else {
            m.indices.iter().flat_map(|i| i.to_le_bytes()).collect()
        };
        let iv = view(&mut bin, &idx_bytes, 34963);
        let n = m.positions.len();
        accessors
            .push(json!({"bufferView": pv, "componentType": 5126, "count": n, "type": "VEC3", "min": lo, "max": hi}));
        accessors.push(json!({"bufferView": nv, "componentType": 5126, "count": n, "type": "VEC3"}));
        accessors.push(json!({"bufferView": iv, "componentType": if small { 5123 } else { 5125 }, "count": m.indices.len(), "type": "SCALAR"}));
        let a = accessors.len();
        meshes.push(json!({"name": part.name, "primitives": [{
            "attributes": {"POSITION": a - 3, "NORMAL": a - 2}, "indices": a - 1, "material": 0
        }]}));
        nodes.push(json!({"name": part.name, "mesh": meshes.len() - 1}));
    }
    let doc = json!({
        "asset": {"version": "2.0", "generator": "Obview 3D"},
        "scene": 0,
        "scenes": [{"nodes": (0..nodes.len()).collect::<Vec<_>>()}],
        "nodes": nodes,
        "meshes": meshes,
        "materials": [{"name": "Acier", "pbrMetallicRoughness": {
            "baseColorFactor": [0.584, 0.604, 0.638, 1.0], "metallicFactor": 0.7, "roughnessFactor": 0.35
        }}],
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": bin.len()}],
    });
    let mut json_bytes = serde_json::to_vec(&doc).expect("JSON");
    pad4(&mut json_bytes, b' ');
    pad4(&mut bin, 0);
    let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend(b"glTF");
    out.extend(2u32.to_le_bytes());
    out.extend((total as u32).to_le_bytes());
    out.extend((json_bytes.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(json_bytes);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(bin);
    out
}

fn f32_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

// ---- OBJ ----

fn obj(model: &Model) -> Vec<u8> {
    let scale = model.unit_m * 1000.0;
    let mut out = Vec::new();
    let _ = writeln!(out, "# Obview 3D · unité : millimètre");
    let mut base = 1usize;
    for part in &model.parts {
        let m = &part.mesh;
        let _ = writeln!(out, "o {}", part.name.replace(['\n', '\r'], " "));
        for p in &m.positions {
            let _ = writeln!(out, "v {} {} {}", p[0] as f64 * scale, p[1] as f64 * scale, p[2] as f64 * scale);
        }
        for n in &m.normals {
            let _ = writeln!(out, "vn {} {} {}", n[0], n[1], n[2]);
        }
        for t in m.indices.chunks_exact(3) {
            let (a, b, c) = (t[0] as usize + base, t[1] as usize + base, t[2] as usize + base);
            let _ = writeln!(out, "f {a}//{a} {b}//{b} {c}//{c}");
        }
        base += m.positions.len();
    }
    out
}

// ---- FBX binaire 7.4 ----

enum Prop {
    I32(i32),
    I64(i64),
    F64(f64),
    Str(Vec<u8>),
    F64s(Vec<f64>),
    I32s(Vec<i32>),
}

struct FbxNode {
    name: &'static str,
    props: Vec<Prop>,
    children: Vec<FbxNode>,
}

fn node(name: &'static str, props: Vec<Prop>) -> FbxNode {
    FbxNode { name, props, children: Vec::new() }
}

impl FbxNode {
    fn with(mut self, children: Vec<FbxNode>) -> Self {
        self.children = children;
        self
    }
}

fn s(v: &str) -> Prop {
    Prop::Str(v.as_bytes().to_vec())
}

fn write_prop(out: &mut Vec<u8>, p: &Prop) {
    match p {
        Prop::I32(v) => {
            out.push(b'I');
            out.extend(v.to_le_bytes());
        }
        Prop::I64(v) => {
            out.push(b'L');
            out.extend(v.to_le_bytes());
        }
        Prop::F64(v) => {
            out.push(b'D');
            out.extend(v.to_le_bytes());
        }
        Prop::Str(b) => {
            out.push(b'S');
            out.extend((b.len() as u32).to_le_bytes());
            out.extend(b);
        }
        Prop::F64s(v) => {
            out.push(b'd');
            out.extend((v.len() as u32).to_le_bytes());
            out.extend(0u32.to_le_bytes());
            out.extend(((v.len() * 8) as u32).to_le_bytes());
            for x in v {
                out.extend(x.to_le_bytes());
            }
        }
        Prop::I32s(v) => {
            out.push(b'i');
            out.extend((v.len() as u32).to_le_bytes());
            out.extend(0u32.to_le_bytes());
            out.extend(((v.len() * 4) as u32).to_le_bytes());
            for x in v {
                out.extend(x.to_le_bytes());
            }
        }
    }
}

const NULL_RECORD: [u8; 13] = [0; 13];

fn write_node(out: &mut Vec<u8>, n: &FbxNode) {
    let start = out.len();
    out.extend([0u8; 12]);
    out.push(n.name.len() as u8);
    out.extend(n.name.as_bytes());
    let props_start = out.len();
    for p in &n.props {
        write_prop(out, p);
    }
    let props_len = out.len() - props_start;
    for c in &n.children {
        write_node(out, c);
    }
    if !n.children.is_empty() || n.props.is_empty() {
        out.extend(NULL_RECORD);
    }
    let end = out.len() as u32;
    out[start..start + 4].copy_from_slice(&end.to_le_bytes());
    out[start + 4..start + 8].copy_from_slice(&(n.props.len() as u32).to_le_bytes());
    out[start + 8..start + 12].copy_from_slice(&(props_len as u32).to_le_bytes());
}

/// Propriété « P » d'un bloc Properties70.
fn p70(name: &str, ty: &str, label: &str, value: Prop) -> FbxNode {
    node("P", vec![s(name), s(ty), s(label), s(""), value])
}

fn fbx(model: &Model) -> Vec<u8> {
    let scale = model.unit_m * 1000.0; // coordonnées écrites en millimètres
    let mut objects = Vec::new();
    let mut connections = Vec::new();
    let mut next_id = 1_000_000i64;
    for part in &model.parts {
        let m = &part.mesh;
        if m.indices.is_empty() {
            continue;
        }
        let (geom_id, model_id) = (next_id, next_id + 1);
        next_id += 2;
        let vertices: Vec<f64> = m.positions.iter().flat_map(|p| p.map(|v| v as f64 * scale)).collect();
        let polys: Vec<i32> =
            m.indices.chunks_exact(3).flat_map(|t| [t[0] as i32, t[1] as i32, -(t[2] as i32) - 1]).collect();
        let normals: Vec<f64> = m.normals.iter().flat_map(|n| n.map(|v| v as f64)).collect();
        let name = part.name.replace('\0', " ");
        let mut geom_name = name.clone().into_bytes();
        geom_name.extend(b"\x00\x01Geometry");
        let mut model_name = name.into_bytes();
        model_name.extend(b"\x00\x01Model");
        objects.push(node("Geometry", vec![Prop::I64(geom_id), Prop::Str(geom_name), s("Mesh")]).with(vec![
            node("Vertices", vec![Prop::F64s(vertices)]),
            node("PolygonVertexIndex", vec![Prop::I32s(polys)]),
            node("GeometryVersion", vec![Prop::I32(124)]),
            node("LayerElementNormal", vec![Prop::I32(0)]).with(vec![
                node("Version", vec![Prop::I32(101)]),
                node("Name", vec![s("")]),
                node("MappingInformationType", vec![s("ByVertice")]),
                node("ReferenceInformationType", vec![s("Direct")]),
                node("Normals", vec![Prop::F64s(normals)]),
            ]),
            node("Layer", vec![Prop::I32(0)]).with(vec![
                node("Version", vec![Prop::I32(100)]),
                node("LayerElement", vec![]).with(vec![
                    node("Type", vec![s("LayerElementNormal")]),
                    node("TypedIndex", vec![Prop::I32(0)]),
                ]),
            ]),
        ]));
        objects.push(node("Model", vec![Prop::I64(model_id), Prop::Str(model_name), s("Mesh")]).with(vec![
            node("Version", vec![Prop::I32(232)]),
            node("Properties70", vec![]).with(vec![p70("DefaultAttributeIndex", "int", "Integer", Prop::I32(0))]),
            node("Culling", vec![s("CullingOff")]),
        ]));
        connections.push(node("C", vec![s("OO"), Prop::I64(model_id), Prop::I64(0)]));
        connections.push(node("C", vec![s("OO"), Prop::I64(geom_id), Prop::I64(model_id)]));
    }
    let count = (objects.len() / 2) as i32;
    let top = vec![
        node("FBXHeaderExtension", vec![]).with(vec![
            node("FBXHeaderVersion", vec![Prop::I32(1003)]),
            node("FBXVersion", vec![Prop::I32(7400)]),
            node("Creator", vec![s("Obview 3D")]),
        ]),
        node("GlobalSettings", vec![]).with(vec![
            node("Version", vec![Prop::I32(1000)]),
            node("Properties70", vec![]).with(vec![
                p70("UpAxis", "int", "Integer", Prop::I32(1)),
                p70("UpAxisSign", "int", "Integer", Prop::I32(1)),
                p70("FrontAxis", "int", "Integer", Prop::I32(2)),
                p70("FrontAxisSign", "int", "Integer", Prop::I32(1)),
                p70("CoordAxis", "int", "Integer", Prop::I32(0)),
                p70("CoordAxisSign", "int", "Integer", Prop::I32(1)),
                // Centimètres par unité du fichier : 0,1 = millimètre.
                p70("UnitScaleFactor", "double", "Number", Prop::F64(0.1)),
                p70("OriginalUnitScaleFactor", "double", "Number", Prop::F64(0.1)),
            ]),
        ]),
        node("Definitions", vec![]).with(vec![
            node("Version", vec![Prop::I32(100)]),
            node("Count", vec![Prop::I32(count * 2 + 1)]),
            node("ObjectType", vec![s("GlobalSettings")]).with(vec![node("Count", vec![Prop::I32(1)])]),
            node("ObjectType", vec![s("Geometry")]).with(vec![node("Count", vec![Prop::I32(count)])]),
            node("ObjectType", vec![s("Model")]).with(vec![node("Count", vec![Prop::I32(count)])]),
        ]),
        node("Objects", vec![]).with(objects),
        node("Connections", vec![]).with(connections),
    ];
    let mut out = Vec::new();
    out.extend(b"Kaydara FBX Binary  \x00");
    out.extend([0x1a, 0x00]);
    out.extend(7400u32.to_le_bytes());
    for n in &top {
        write_node(&mut out, n);
    }
    out.extend(NULL_RECORD);
    // Pied de fichier : identifiant, alignement sur 16 octets, version, zéros, signature finale.
    out.extend([0xfa, 0xbc, 0xab, 0x09, 0xd0, 0xc8, 0xd4, 0x66, 0xb1, 0x76, 0xfb, 0x83, 0x1c, 0xf7, 0x26, 0x7e]);
    out.extend([0u8; 4]);
    let pad = (16 - out.len() % 16) % 16;
    out.extend(std::iter::repeat_n(0u8, pad));
    out.extend(7400u32.to_le_bytes());
    out.extend([0u8; 120]);
    out.extend([0xf8, 0x5a, 0x8c, 0x6a, 0xde, 0xf5, 0xd9, 0x7e, 0xec, 0xe9, 0x0c, 0xe3, 0x75, 0x8f, 0x29, 0x0b]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use obv_io::{Mesh, Part};

    fn model() -> Model {
        let mesh = Mesh {
            positions: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [0.0, 0.0, 10.0]],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            indices: vec![0, 1, 2, 0, 1, 3],
        };
        Model { parts: vec![Part { name: "Pièce A".into(), mesh }], unit_m: 0.001 }
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join("obv-mesh-essais");
        std::fs::create_dir_all(&d).unwrap();
        d.join(name)
    }

    #[test]
    fn glb_relu() {
        let p = tmp("a.glb");
        std::fs::write(&p, encode(&model(), Format::Glb)).unwrap();
        let m = obv_io::read_glb(&p).unwrap();
        assert_eq!(m.parts.len(), 1);
        assert_eq!(m.parts[0].name, "Pièce A");
        assert_eq!(m.triangle_count(), 2);
        // Mètres dans le fichier : 10 mm = 0,01 m.
        assert!(m.parts[0].mesh.positions.iter().any(|p| (p[0] - 0.01).abs() < 1e-7));
    }

    #[test]
    fn obj_relu() {
        let p = tmp("a.obj");
        std::fs::write(&p, encode(&model(), Format::Obj)).unwrap();
        let m = obv_io::read_obj(&p).unwrap();
        assert_eq!(m.parts[0].name, "Pièce A");
        assert_eq!(m.triangle_count(), 2);
        assert!(m.parts[0].mesh.positions.iter().any(|p| (p[1] - 10.0).abs() < 1e-5));
    }

    #[test]
    fn fbx_relu_par_ufbx() {
        let p = tmp("a.fbx");
        std::fs::write(&p, encode(&model(), Format::Fbx)).unwrap();
        let m = obv_io::read_fbx(&p).unwrap();
        assert_eq!(m.parts.len(), 1);
        assert_eq!(m.parts[0].name, "Pièce A");
        assert_eq!(m.triangle_count(), 2);
        // Lu en mètres : 10 mm = 0,01 m, axes inchangés (Y vers le haut).
        let mesh = &m.parts[0].mesh;
        assert!(mesh.positions.iter().any(|p| (p[1] - 0.01).abs() < 1e-6), "{:?}", mesh.positions);
        assert!(mesh.positions.iter().any(|p| (p[2] - 0.01).abs() < 1e-6), "{:?}", mesh.positions);
    }
}
