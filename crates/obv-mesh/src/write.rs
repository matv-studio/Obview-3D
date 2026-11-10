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

/// Compression du GLB (D-29).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Compression {
    /// Flottants : lisible partout.
    None,
    /// Attributs quantifiés (`KHR_mesh_quantization`) : environ deux fois plus léger, lu par Blender, Unity,
    /// Unreal, three.js, Babylon.js.
    #[default]
    Quantize,
    /// Quantification et codec meshoptimizer (`EXT_meshopt_compression`, exigée) : le plus léger et le plus rapide
    /// à décoder, pour le web (three.js, Babylon.js) ; Blender ne la lit pas.
    Meshopt,
}

impl Compression {
    pub const ALL: [Compression; 3] = [Compression::None, Compression::Quantize, Compression::Meshopt];
}

/// Options d'écriture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WriteOptions {
    pub format: Format,
    /// GLB : compression des géométries.
    pub compression: Compression,
    /// Écart accepté (unités du modèle) : borne la quantification des positions (0 : positions en flottants).
    pub tolerance: f32,
}

/// Fichier produit : nom relatif (le premier est le fichier principal, nommé d'après `stem`) et contenu.
pub struct OutputFile {
    pub name: String,
    pub data: Vec<u8>,
}

/// Encode le modèle. `stem` : nom de base des fichiers (sans extension). OBJ ajoute un `.mtl` et les images.
pub fn encode(model: &Model, opts: WriteOptions, stem: &str) -> Vec<OutputFile> {
    match opts.format {
        Format::Glb => vec![OutputFile { name: format!("{stem}.glb"), data: glb(model, opts) }],
        Format::Obj => obj(model, stem),
        Format::Fbx => vec![OutputFile { name: format!("{stem}.fbx"), data: fbx(model) }],
    }
}

// ---- GLB ----

fn pad4(v: &mut Vec<u8>, byte: u8) {
    while !v.len().is_multiple_of(4) {
        v.push(byte);
    }
}

/// Accumulateur du tampon binaire : vues simples, ou compressées (avec tampon de repli décrit, non écrit).
struct Bin {
    data: Vec<u8>,
    views: Vec<serde_json::Value>,
    compress: bool,
    /// Taille du tampon de repli (données décompressées), pour les vues compressées.
    fallback_len: usize,
}

impl Bin {
    fn raw(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        use serde_json::json;
        pad4(&mut self.data, 0);
        let offset = self.data.len();
        self.data.extend_from_slice(bytes);
        let mut v = json!({"buffer": 0, "byteOffset": offset, "byteLength": bytes.len()});
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        self.views.push(v);
        self.views.len() - 1
    }

    /// Attributs de sommets : `count` éléments de `stride` octets.
    fn attributes(&mut self, bytes: &[u8], stride: usize, count: usize, filter: &str) -> usize {
        use serde_json::json;
        if !self.compress {
            let i = self.raw(bytes, Some(34962));
            self.views[i]["byteStride"] = json!(stride);
            return i;
        }
        let mut enc = vec![
            0u8;
            unsafe {
                // SAFETY: fonction pure de taille.
                meshopt::ffi::meshopt_encodeVertexBufferBound(count, stride)
            }
        ];
        let n = unsafe {
            // SAFETY: `bytes` contient `count × stride` octets ; `enc` a la taille bornée ci-dessus.
            meshopt::ffi::meshopt_encodeVertexBuffer(enc.as_mut_ptr(), enc.len(), bytes.as_ptr().cast(), count, stride)
        };
        enc.truncate(n);
        self.compressed(&enc, bytes.len(), stride, count, "ATTRIBUTES", filter, Some(34962))
    }

    /// Indices de triangles (`stride` 2 ou 4 octets).
    fn triangles(&mut self, indices: &[u32], vertex_count: usize, stride: usize) -> usize {
        let raw: Vec<u8> = if stride == 2 {
            indices.iter().flat_map(|i| (*i as u16).to_le_bytes()).collect()
        } else {
            indices.iter().flat_map(|i| i.to_le_bytes()).collect()
        };
        if !self.compress {
            return self.raw(&raw, Some(34963));
        }
        let enc = meshopt::encode_index_buffer(indices, vertex_count).expect("indices");
        self.compressed(&enc, raw.len(), stride, indices.len(), "TRIANGLES", "NONE", Some(34963))
    }

    #[allow(clippy::too_many_arguments)]
    fn compressed(
        &mut self,
        enc: &[u8],
        raw_len: usize,
        stride: usize,
        count: usize,
        mode: &str,
        filter: &str,
        target: Option<u32>,
    ) -> usize {
        use serde_json::json;
        pad4(&mut self.data, 0);
        let offset = self.data.len();
        self.data.extend_from_slice(enc);
        self.fallback_len = (self.fallback_len + 3) & !3;
        let fb = self.fallback_len;
        self.fallback_len += raw_len;
        let mut ext = json!({"buffer": 0, "byteOffset": offset, "byteLength": enc.len(), "byteStride": stride,
            "count": count, "mode": mode});
        if filter != "NONE" {
            ext["filter"] = json!(filter);
        }
        let mut v = json!({"buffer": 1, "byteOffset": fb, "byteLength": raw_len,
            "extensions": {"EXT_meshopt_compression": ext}});
        if mode == "ATTRIBUTES" {
            v["byteStride"] = json!(stride);
        }
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        self.views.push(v);
        self.views.len() - 1
    }
}

fn glb(model: &Model, opts: WriteOptions) -> Vec<u8> {
    use serde_json::{Value, json};
    let to_m = model.unit_m as f32;
    let meshopt_on = opts.compression == Compression::Meshopt;
    let quant_on = opts.compression != Compression::None;
    if meshopt_on {
        unsafe {
            // SAFETY: réglage global de version, sans effet sur la mémoire. Version 0 du codec de sommets : celle
            // de `EXT_meshopt_compression` (la version 1 relève de `KHR_meshopt_compression`).
            meshopt::ffi::meshopt_encodeVertexVersion(0);
            meshopt::ffi::meshopt_encodeIndexVersion(1);
        }
    }
    let mut bin = Bin { data: Vec::new(), views: Vec::new(), compress: meshopt_on, fallback_len: 0 };
    let (mut accessors, mut meshes, mut nodes) = (Vec::<Value>::new(), Vec::<Value>::new(), Vec::<Value>::new());
    let mut used_quantization = false;

    // Images, textures, matériaux.
    let mut images = Vec::new();
    let mut textures = Vec::new();
    let mut uses_webp = false;
    for (i, t) in model.textures.iter().enumerate() {
        let view = bin.raw(&t.data, None);
        images.push(json!({"name": t.name, "bufferView": view, "mimeType": t.mime}));
        if t.mime == "image/webp" {
            uses_webp = true;
            textures.push(json!({"sampler": 0, "extensions": {"EXT_texture_webp": {"source": i}}}));
        } else {
            textures.push(json!({"sampler": 0, "source": i}));
        }
    }
    let tex_ref = |t: Option<u32>| t.map(|i| json!({"index": i}));
    let mut materials: Vec<Value> = model
        .materials
        .iter()
        .map(|m| {
            let mut pbr = json!({"baseColorFactor": m.base_color, "metallicFactor": m.metallic,
                "roughnessFactor": m.roughness});
            if let Some(t) = tex_ref(m.base_color_texture) {
                pbr["baseColorTexture"] = t;
            }
            if let Some(t) = tex_ref(m.metallic_roughness_texture) {
                pbr["metallicRoughnessTexture"] = t;
            }
            let mut v = json!({"name": m.name, "pbrMetallicRoughness": pbr});
            if m.emissive.iter().any(|c| *c > 0.0) {
                v["emissiveFactor"] = json!(m.emissive.map(|c| c.min(1.0)));
            }
            for (key, t) in [
                ("normalTexture", m.normal_texture),
                ("occlusionTexture", m.occlusion_texture),
                ("emissiveTexture", m.emissive_texture),
            ] {
                if let Some(t) = tex_ref(t) {
                    v[key] = t;
                }
            }
            if m.double_sided {
                v["doubleSided"] = json!(true);
            }
            if m.blend {
                v["alphaMode"] = json!("BLEND");
            }
            v
        })
        .collect();
    let default_material = materials.len();
    materials.push(json!({"name": "Acier", "pbrMetallicRoughness": {
        "baseColorFactor": [0.584, 0.604, 0.638, 1.0], "metallicFactor": 0.7, "roughnessFactor": 0.35
    }}));

    for part in &model.parts {
        let m = &part.mesh;
        if m.indices.is_empty() {
            continue;
        }
        let n = m.positions.len();
        let pos: Vec<[f32; 3]> = m.positions.iter().map(|p| p.map(|v| v * to_m)).collect();
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &pos {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        // Quantification des positions sur 16 bits, échelle uniforme (les normales ne sont pas déformées),
        // seulement si le pas reste sous le quart de l'écart accepté.
        let extent = (0..3).map(|i| hi[i] - lo[i]).fold(0.0f32, f32::max).max(1e-12);
        let step = extent / 65535.0;
        let quantize = quant_on && opts.tolerance > 0.0 && step <= opts.tolerance * to_m * 0.25;
        let mut node = json!({"name": part.name, "mesh": meshes.len()});
        let pos_acc = if quantize {
            used_quantization = true;
            let q: Vec<u8> = pos
                .iter()
                .flat_map(|p| {
                    let c = |i: usize| (((p[i] - lo[i]) / step).round().clamp(0.0, 65535.0)) as u16;
                    [c(0), c(1), c(2), 0].into_iter().flat_map(u16::to_le_bytes)
                })
                .collect();
            let v = bin.attributes(&q, 8, n, "NONE");
            node["translation"] = json!(lo);
            node["scale"] = json!([step, step, step]);
            let qmax: Vec<f32> = (0..3).map(|i| ((hi[i] - lo[i]) / step).round()).collect();
            json!({"bufferView": v, "componentType": 5123, "count": n, "type": "VEC3", "min": [0, 0, 0], "max": qmax})
        } else {
            let raw: Vec<u8> = pos.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
            let v = bin.attributes(&raw, 12, n, "NONE");
            json!({"bufferView": v, "componentType": 5126, "count": n, "type": "VEC3", "min": lo, "max": hi})
        };
        accessors.push(pos_acc);
        let pos_i = accessors.len() - 1;
        // Normales : octaédriques sur 8 bits (filtre meshoptimizer) avec meshopt, entiers signés normalisés sur
        // 8 bits en quantification seule, sinon flottants.
        let nrm_acc = if meshopt_on {
            used_quantization = true;
            let n4: Vec<[f32; 4]> = m.normals.iter().map(|v| [v[0], v[1], v[2], 0.0]).collect();
            let enc: Vec<[i8; 4]> = meshopt::encode_filter_oct(&n4, 8);
            let raw: Vec<u8> = enc.iter().flatten().map(|b| *b as u8).collect();
            let v = bin.attributes(&raw, 4, n, "OCTAHEDRAL");
            json!({"bufferView": v, "componentType": 5120, "normalized": true, "count": n, "type": "VEC3"})
        } else if quant_on {
            used_quantization = true;
            let raw: Vec<u8> = m
                .normals
                .iter()
                .flat_map(|v| {
                    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-12);
                    let c = |x: f32| ((x / l) * 127.0).round().clamp(-127.0, 127.0) as i8 as u8;
                    [c(v[0]), c(v[1]), c(v[2]), 0]
                })
                .collect();
            let v = bin.attributes(&raw, 4, n, "NONE");
            json!({"bufferView": v, "componentType": 5120, "normalized": true, "count": n, "type": "VEC3"})
        } else {
            let raw: Vec<u8> = m.normals.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
            let v = bin.attributes(&raw, 12, n, "NONE");
            json!({"bufferView": v, "componentType": 5126, "count": n, "type": "VEC3"})
        };
        accessors.push(nrm_acc);
        let nrm_i = accessors.len() - 1;
        let uv_i = if m.uvs.len() == n {
            let in_unit = m.uvs.iter().flatten().all(|v| (0.0..=1.0).contains(v));
            let acc = if quant_on && in_unit {
                used_quantization = true;
                let raw: Vec<u8> = m
                    .uvs
                    .iter()
                    .flat_map(|t| t.map(|v| (v * 65535.0).round() as u16))
                    .flat_map(u16::to_le_bytes)
                    .collect();
                let v = bin.attributes(&raw, 4, n, "NONE");
                json!({"bufferView": v, "componentType": 5123, "normalized": true, "count": n, "type": "VEC2"})
            } else {
                let raw: Vec<u8> = m.uvs.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
                let v = bin.attributes(&raw, 8, n, "NONE");
                json!({"bufferView": v, "componentType": 5126, "count": n, "type": "VEC2"})
            };
            accessors.push(acc);
            Some(accessors.len() - 1)
        } else {
            None
        };
        let small = n <= u16::MAX as usize;
        let mut prims = Vec::new();
        for g in m.groups() {
            let idx = &m.indices[g.first as usize..(g.first + g.count) as usize];
            let iv = bin.triangles(idx, n, if small { 2 } else { 4 });
            accessors.push(json!({"bufferView": iv, "componentType": if small { 5123 } else { 5125 },
                "count": idx.len(), "type": "SCALAR"}));
            let mut attrs = json!({"POSITION": pos_i, "NORMAL": nrm_i});
            if let Some(u) = uv_i {
                attrs["TEXCOORD_0"] = json!(u);
            }
            let mat = g.material.map(|m| m as usize).filter(|m| *m < default_material).unwrap_or(default_material);
            prims.push(json!({"attributes": attrs, "indices": accessors.len() - 1, "material": mat}));
        }
        meshes.push(json!({"name": part.name, "primitives": prims}));
        nodes.push(node);
    }
    let mut ext_used: Vec<&str> = Vec::new();
    if meshopt_on {
        ext_used.push("EXT_meshopt_compression");
    }
    if used_quantization {
        ext_used.push("KHR_mesh_quantization");
    }
    if uses_webp {
        ext_used.push("EXT_texture_webp");
    }
    let mut buffers = vec![json!({"byteLength": 0})];
    if meshopt_on {
        buffers.push(json!({"byteLength": bin.fallback_len.max(4),
            "extensions": {"EXT_meshopt_compression": {"fallback": true}}}));
    }
    let mut doc = json!({
        "asset": {"version": "2.0", "generator": "Obview 3D"},
        "scene": 0,
        "scenes": [{"nodes": (0..nodes.len()).collect::<Vec<_>>()}],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "accessors": accessors,
        "bufferViews": bin.views,
    });
    if !images.is_empty() {
        doc["images"] = json!(images);
        doc["textures"] = json!(textures);
        doc["samplers"] = json!([{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}]);
    }
    if !ext_used.is_empty() {
        doc["extensionsUsed"] = json!(ext_used);
        // Sans le décodeur, le fichier est illisible : extensions exigées.
        let required: Vec<&str> = ext_used.iter().copied().filter(|e| *e != "EXT_texture_webp").collect();
        doc["extensionsRequired"] = json!(required);
    }
    let mut data = bin.data;
    pad4(&mut data, 0);
    buffers[0]["byteLength"] = json!(data.len());
    doc["buffers"] = json!(buffers);
    let mut json_bytes = serde_json::to_vec(&doc).expect("JSON");
    pad4(&mut json_bytes, b' ');
    let total = 12 + 8 + json_bytes.len() + 8 + data.len();
    let mut out = Vec::with_capacity(total);
    out.extend(b"glTF");
    out.extend(2u32.to_le_bytes());
    out.extend((total as u32).to_le_bytes());
    out.extend((json_bytes.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(json_bytes);
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(data);
    out
}

// ---- OBJ ----

/// Nom de fichier sûr pour une image ou un matériau.
fn safe(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    if s.is_empty() { "sans_nom".into() } else { s }
}

fn texture_file_names(model: &Model, stem: &str) -> Vec<String> {
    let mut used = std::collections::HashSet::new();
    model
        .textures
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let mut n = format!("{stem}_{}.{}", safe(&t.name), t.extension());
            if !used.insert(n.clone()) {
                n = format!("{stem}_{}_{i}.{}", safe(&t.name), t.extension());
                used.insert(n.clone());
            }
            n
        })
        .collect()
}

fn obj(model: &Model, stem: &str) -> Vec<OutputFile> {
    let scale = model.unit_m * 1000.0;
    let mut out = Vec::new();
    let has_mtl = !model.materials.is_empty();
    let _ = writeln!(out, "# Obview 3D · unité : millimètre");
    if has_mtl {
        let _ = writeln!(out, "mtllib {stem}.mtl");
    }
    let mat_names: Vec<String> =
        model.materials.iter().enumerate().map(|(i, m)| format!("{}_{i}", safe(&m.name))).collect();
    let (mut base_v, mut base_t) = (1usize, 1usize);
    for part in &model.parts {
        let m = &part.mesh;
        let _ = writeln!(out, "o {}", part.name.replace(['\n', '\r'], " "));
        for p in &m.positions {
            let _ = writeln!(out, "v {} {} {}", p[0] as f64 * scale, p[1] as f64 * scale, p[2] as f64 * scale);
        }
        let uv = m.uvs.len() == m.positions.len() && !m.uvs.is_empty();
        if uv {
            // OBJ : origine des coordonnées de texture en bas de l'image.
            for t in &m.uvs {
                let _ = writeln!(out, "vt {} {}", t[0], 1.0 - t[1]);
            }
        }
        for n in &m.normals {
            let _ = writeln!(out, "vn {} {} {}", n[0], n[1], n[2]);
        }
        for g in m.groups() {
            if let Some(mi) = g.material
                && let Some(name) = mat_names.get(mi as usize)
            {
                let _ = writeln!(out, "usemtl {name}");
            }
            for t in m.indices[g.first as usize..(g.first + g.count) as usize].chunks_exact(3) {
                let c = |i: u32| {
                    let v = i as usize + base_v;
                    if uv { format!("{v}/{}/{v}", i as usize + base_t) } else { format!("{v}//{v}") }
                };
                let _ = writeln!(out, "f {} {} {}", c(t[0]), c(t[1]), c(t[2]));
            }
        }
        base_v += m.positions.len();
        if uv {
            base_t += m.uvs.len();
        }
    }
    let mut files = vec![OutputFile { name: format!("{stem}.obj"), data: out }];
    if has_mtl {
        let tex_names = texture_file_names(model, stem);
        let mut mtl = Vec::new();
        let _ = writeln!(mtl, "# Obview 3D");
        for (m, name) in model.materials.iter().zip(&mat_names) {
            let s = |c: f32| obv_io::linear_to_srgb(c);
            let _ = writeln!(mtl, "\nnewmtl {name}");
            let _ = writeln!(mtl, "Kd {} {} {}", s(m.base_color[0]), s(m.base_color[1]), s(m.base_color[2]));
            let _ = writeln!(mtl, "d {}", m.base_color[3]);
            let _ = writeln!(mtl, "Pr {}\nPm {}", m.roughness, m.metallic);
            let ns = (2.0 / (m.roughness * m.roughness).max(1e-4) - 2.0).clamp(0.0, 1000.0);
            let _ = writeln!(mtl, "Ns {ns}");
            if m.emissive.iter().any(|c| *c > 0.0) {
                let _ = writeln!(mtl, "Ke {} {} {}", m.emissive[0], m.emissive[1], m.emissive[2]);
            }
            for (key, t) in [
                ("map_Kd", m.base_color_texture),
                ("map_Pr", m.metallic_roughness_texture),
                ("norm", m.normal_texture),
                ("map_Ke", m.emissive_texture),
            ] {
                if let Some(t) = t.and_then(|t| tex_names.get(t as usize)) {
                    let _ = writeln!(mtl, "{key} {t}");
                }
            }
        }
        files.push(OutputFile { name: format!("{stem}.mtl"), data: mtl });
        for (t, name) in model.textures.iter().zip(tex_names) {
            files.push(OutputFile { name, data: t.data.clone() });
        }
    }
    files
}

// ---- FBX binaire 7.4 ----

enum Prop {
    I32(i32),
    I64(i64),
    F64(f64),
    Str(Vec<u8>),
    F64s(Vec<f64>),
    I32s(Vec<i32>),
    /// Octets bruts (contenu d'une image intégrée).
    Raw(Vec<u8>),
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
        Prop::Raw(b) => {
            out.push(b'R');
            out.extend((b.len() as u32).to_le_bytes());
            out.extend(b);
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

/// Propriété couleur « P » (trois composantes).
fn p70_color(name: &str, c: [f64; 3]) -> FbxNode {
    node("P", vec![s(name), s("Color"), s(""), s("A"), Prop::F64(c[0]), Prop::F64(c[1]), Prop::F64(c[2])])
}

/// Nom d'objet FBX binaire : « nom\0\x01Classe ».
fn fbx_name(name: &str, class: &str) -> Prop {
    let mut b = name.replace('\0', " ").into_bytes();
    b.extend(b"\x00\x01");
    b.extend(class.as_bytes());
    Prop::Str(b)
}

fn fbx(model: &Model) -> Vec<u8> {
    let scale = model.unit_m * 1000.0; // coordonnées écrites en millimètres
    let mut objects = Vec::new();
    let mut connections = Vec::new();
    let mut next_id = 1_000_000i64;
    let mut id = || {
        next_id += 1;
        next_id
    };
    // Images intégrées (Video, contenu brut) et textures.
    let tex_names = texture_file_names(model, "texture");
    let mut tex_ids = Vec::new();
    for (t, file) in model.textures.iter().zip(&tex_names) {
        let (video_id, tex_id) = (id(), id());
        objects.push(node("Video", vec![Prop::I64(video_id), fbx_name(&t.name, "Video"), s("Clip")]).with(vec![
            node("Type", vec![s("Clip")]),
            node("FileName", vec![s(file)]),
            node("RelativeFilename", vec![s(file)]),
            node("Content", vec![Prop::Raw(t.data.clone())]),
        ]));
        objects.push(node("Texture", vec![Prop::I64(tex_id), fbx_name(&t.name, "Texture"), s("")]).with(vec![
            node("Type", vec![s("TextureVideoClip")]),
            node("Version", vec![Prop::I32(202)]),
            node("TextureName", vec![fbx_name(&t.name, "Texture")]),
            node("Media", vec![fbx_name(&t.name, "Video")]),
            node("FileName", vec![s(file)]),
            node("RelativeFilename", vec![s(file)]),
        ]));
        connections.push(node("C", vec![s("OO"), Prop::I64(video_id), Prop::I64(tex_id)]));
        tex_ids.push(tex_id);
    }
    // Matériaux (modèle Phong, le plus largement lu ; métal et rugosité en propriétés supplémentaires).
    let mut mat_ids = Vec::new();
    let mut materials: Vec<obv_io::Material> = model.materials.clone();
    materials.push(obv_io::Material {
        name: "Acier".into(),
        base_color: [0.584, 0.604, 0.638, 1.0],
        metallic: 0.7,
        roughness: 0.35,
        ..Default::default()
    });
    for m in &materials {
        let mid = id();
        let c = m.base_color.map(|v| v as f64);
        let shininess = (2.0 / (m.roughness as f64 * m.roughness as f64).max(1e-4) - 2.0).clamp(0.0, 1000.0);
        objects.push(node("Material", vec![Prop::I64(mid), fbx_name(&m.name, "Material"), s("")]).with(vec![
            node("Version", vec![Prop::I32(102)]),
            node("ShadingModel", vec![s("phong")]),
            node("MultiLayer", vec![Prop::I32(0)]),
            node("Properties70", vec![]).with(vec![
                p70_color("DiffuseColor", [c[0], c[1], c[2]]),
                p70("DiffuseFactor", "double", "Number", Prop::F64(1.0)),
                p70_color("EmissiveColor", m.emissive.map(|v| v as f64)),
                p70("Opacity", "double", "Number", Prop::F64(c[3])),
                // Transparence au sens Phong (lue par la plupart des logiciels, ufbx compris).
                p70_color("TransparentColor", [1.0, 1.0, 1.0]),
                p70("TransparencyFactor", "double", "Number", Prop::F64(1.0 - c[3])),
                p70("ShininessExponent", "double", "Number", Prop::F64(shininess)),
                p70("Metallic", "double", "Number", Prop::F64(m.metallic as f64)),
                p70("Roughness", "double", "Number", Prop::F64(m.roughness as f64)),
            ]),
        ]));
        for (slot, t) in [
            ("DiffuseColor", m.base_color_texture),
            ("NormalMap", m.normal_texture),
            ("EmissiveColor", m.emissive_texture),
        ] {
            if let Some(tid) = t.and_then(|t| tex_ids.get(t as usize)) {
                connections.push(node("C", vec![s("OP"), Prop::I64(*tid), Prop::I64(mid), s(slot)]));
            }
        }
        mat_ids.push(mid);
    }
    let default_mat = mat_ids.len() - 1;
    for part in &model.parts {
        let m = &part.mesh;
        if m.indices.is_empty() {
            continue;
        }
        let (geom_id, model_id) = (id(), id());
        let vertices: Vec<f64> = m.positions.iter().flat_map(|p| p.map(|v| v as f64 * scale)).collect();
        let polys: Vec<i32> =
            m.indices.chunks_exact(3).flat_map(|t| [t[0] as i32, t[1] as i32, -(t[2] as i32) - 1]).collect();
        let normals: Vec<f64> = m.normals.iter().flat_map(|n| n.map(|v| v as f64)).collect();
        // Matériaux de la pièce, dans l'ordre de leurs liaisons au modèle ; indice local par polygone.
        let mut local: Vec<usize> = Vec::new();
        let mut per_poly: Vec<i32> = Vec::with_capacity(m.triangle_count());
        for g in m.groups() {
            let global = g.material.map(|v| v as usize).filter(|v| *v < default_mat).unwrap_or(default_mat);
            let li = match local.iter().position(|x| *x == global) {
                Some(i) => i,
                None => {
                    local.push(global);
                    local.len() - 1
                }
            };
            per_poly.extend(std::iter::repeat_n(li as i32, g.count as usize / 3));
        }
        let mut layers = vec![
            node("Version", vec![Prop::I32(100)]),
            node("LayerElement", vec![])
                .with(vec![node("Type", vec![s("LayerElementNormal")]), node("TypedIndex", vec![Prop::I32(0)])]),
            node("LayerElement", vec![])
                .with(vec![node("Type", vec![s("LayerElementMaterial")]), node("TypedIndex", vec![Prop::I32(0)])]),
        ];
        let mut geom = vec![
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
            node("LayerElementMaterial", vec![Prop::I32(0)]).with(vec![
                node("Version", vec![Prop::I32(101)]),
                node("Name", vec![s("")]),
                node("MappingInformationType", vec![s("ByPolygon")]),
                node("ReferenceInformationType", vec![s("IndexToDirect")]),
                node("Materials", vec![Prop::I32s(per_poly)]),
            ]),
        ];
        if m.uvs.len() == m.positions.len() && !m.uvs.is_empty() {
            // FBX : origine des coordonnées de texture en bas de l'image.
            let uv: Vec<f64> = m.uvs.iter().flat_map(|t| [t[0] as f64, 1.0 - t[1] as f64]).collect();
            geom.push(node("LayerElementUV", vec![Prop::I32(0)]).with(vec![
                node("Version", vec![Prop::I32(101)]),
                node("Name", vec![s("UVMap")]),
                node("MappingInformationType", vec![s("ByVertice")]),
                node("ReferenceInformationType", vec![s("Direct")]),
                node("UV", vec![Prop::F64s(uv)]),
            ]));
            layers.push(
                node("LayerElement", vec![])
                    .with(vec![node("Type", vec![s("LayerElementUV")]), node("TypedIndex", vec![Prop::I32(0)])]),
            );
        }
        geom.push(node("Layer", vec![Prop::I32(0)]).with(layers));
        objects
            .push(node("Geometry", vec![Prop::I64(geom_id), fbx_name(&part.name, "Geometry"), s("Mesh")]).with(geom));
        objects.push(node("Model", vec![Prop::I64(model_id), fbx_name(&part.name, "Model"), s("Mesh")]).with(vec![
            node("Version", vec![Prop::I32(232)]),
            node("Properties70", vec![]).with(vec![p70("DefaultAttributeIndex", "int", "Integer", Prop::I32(0))]),
            node("Culling", vec![s("CullingOff")]),
        ]));
        connections.push(node("C", vec![s("OO"), Prop::I64(model_id), Prop::I64(0)]));
        connections.push(node("C", vec![s("OO"), Prop::I64(geom_id), Prop::I64(model_id)]));
        for g in local {
            connections.push(node("C", vec![s("OO"), Prop::I64(mat_ids[g]), Prop::I64(model_id)]));
        }
    }
    let count = model.parts.iter().filter(|p| !p.mesh.indices.is_empty()).count() as i32;
    let (n_mat, n_tex) = (mat_ids.len() as i32, tex_ids.len() as i32);
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
            node("Count", vec![Prop::I32(count * 2 + n_mat + n_tex * 2 + 1)]),
            node("ObjectType", vec![s("GlobalSettings")]).with(vec![node("Count", vec![Prop::I32(1)])]),
            node("ObjectType", vec![s("Geometry")]).with(vec![node("Count", vec![Prop::I32(count)])]),
            node("ObjectType", vec![s("Model")]).with(vec![node("Count", vec![Prop::I32(count)])]),
            node("ObjectType", vec![s("Material")]).with(vec![node("Count", vec![Prop::I32(n_mat)])]),
            node("ObjectType", vec![s("Texture")]).with(vec![node("Count", vec![Prop::I32(n_tex)])]),
            node("ObjectType", vec![s("Video")]).with(vec![node("Count", vec![Prop::I32(n_tex)])]),
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
    use obv_io::{Material, Mesh, Part, SubMesh, Texture};

    /// Deux triangles, deux matériaux (l'un texturé), coordonnées de texture.
    fn model() -> Model {
        let mesh = Mesh {
            positions: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [0.0, 0.0, 10.0]],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [0.5, 0.5]],
            indices: vec![0, 1, 2, 0, 1, 3],
            submeshes: vec![
                SubMesh { first: 0, count: 3, material: Some(0) },
                SubMesh { first: 3, count: 3, material: Some(1) },
            ],
        };
        Model {
            parts: vec![Part { name: "Pièce A".into(), mesh }],
            unit_m: 0.001,
            materials: vec![
                Material {
                    name: "Laiton".into(),
                    base_color: [0.8, 0.5, 0.1, 1.0],
                    base_color_texture: Some(0),
                    ..Default::default()
                },
                Material { name: "Verre".into(), base_color: [0.9, 0.9, 1.0, 0.3], blend: true, ..Default::default() },
            ],
            textures: vec![Texture {
                name: "grain".into(),
                mime: "image/png".into(),
                data: vec![0x89, b'P', b'N', b'G', 9, 9],
            }],
        }
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join("obv-mesh-essais");
        std::fs::create_dir_all(&d).unwrap();
        d.join(name)
    }

    fn write_all(files: Vec<OutputFile>) -> std::path::PathBuf {
        let main = tmp(&files[0].name);
        for f in &files {
            std::fs::write(tmp(&f.name), &f.data).unwrap();
        }
        main
    }

    fn check(m: &Model, scale_m: f32) {
        assert_eq!(m.parts.len(), 1);
        assert_eq!(m.parts[0].name, "Pièce A");
        assert_eq!(m.triangle_count(), 2);
        let mesh = &m.parts[0].mesh;
        assert!(mesh.positions.iter().any(|p| (p[1] - 10.0 * scale_m).abs() < 1e-5 * scale_m.max(1e-3) * 1e3));
        assert_eq!(mesh.uvs.len(), mesh.positions.len(), "coordonnées de texture perdues");
        let mats: Vec<_> = mesh.submeshes.iter().filter_map(|g| g.material).collect();
        assert_eq!(mats.len(), 2, "plages par matériau perdues : {:?}", mesh.submeshes);
        let laiton = m.materials.iter().find(|x| x.name.starts_with("Laiton")).expect("matériau Laiton");
        assert!((laiton.base_color[0] - 0.8).abs() < 0.01, "{:?}", laiton.base_color);
        let tex = laiton.base_color_texture.expect("texture de couleur");
        assert_eq!(m.textures[tex as usize].data, vec![0x89, b'P', b'N', b'G', 9, 9]);
        assert!(m.materials.iter().any(|x| x.name.starts_with("Verre") && x.base_color[3] < 0.5));
    }

    #[test]
    fn glb_relu_avec_et_sans_compression() {
        for compression in Compression::ALL {
            let opts = WriteOptions { format: Format::Glb, compression, tolerance: 0.01 };
            let p = write_all(encode(&model(), opts, &format!("{compression:?}")));
            let m = obv_io::read_glb(&p).unwrap();
            check(&m, 0.001);
            // Seul meshopt exige une extension que Blender ignore ; la quantification reste lisible partout.
            let bytes = std::fs::read(&p).unwrap();
            let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
            let doc: serde_json::Value = serde_json::from_slice(&bytes[20..20 + len]).unwrap();
            let required: Vec<&str> = doc["extensionsRequired"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            match compression {
                Compression::None => assert!(required.is_empty(), "{required:?}"),
                Compression::Quantize => assert_eq!(required, ["KHR_mesh_quantization"]),
                Compression::Meshopt => assert!(required.contains(&"EXT_meshopt_compression")),
            }
            // Positions quantifiées : à moins d'un pas (10 mm / 65535) près.
            let mesh = &m.parts[0].mesh;
            assert!(mesh.positions.iter().any(|q| (q[0] - 0.01).abs() < 1e-6), "{:?}", mesh.positions);
        }
    }

    #[test]
    fn obj_relu() {
        let p = write_all(encode(
            &model(),
            WriteOptions { format: Format::Obj, compression: Compression::None, tolerance: 0.0 },
            "o",
        ));
        let m = obv_io::read_obj(&p).unwrap();
        check(&m, 1.0);
    }

    #[test]
    fn fbx_relu_par_ufbx() {
        let p = write_all(encode(
            &model(),
            WriteOptions { format: Format::Fbx, compression: Compression::None, tolerance: 0.0 },
            "f",
        ));
        let m = obv_io::read_fbx(&p).unwrap();
        check(&m, 0.001);
        let mesh = &m.parts[0].mesh;
        // Axes inchangés (Y vers le haut) et UV relus dans le même sens.
        assert!(mesh.positions.iter().any(|q| (q[2] - 0.01).abs() < 1e-6));
        assert!(mesh.uvs.iter().any(|t| (t[0] - 0.5).abs() < 1e-6 && (t[1] - 0.5).abs() < 1e-6));
        assert!(mesh.uvs.iter().any(|t| (t[0] - 1.0).abs() < 1e-6 && t[1].abs() < 1e-6), "{:?}", mesh.uvs);
    }
}
