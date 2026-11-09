//! glTF 2.0 binaire : en-tête, bloc JSON, bloc binaire ; nœuds et transformations, accesseurs avec pas,
//! matériaux métal-rugosité, textures (intégrées, en `data:` ou en fichiers voisins), coordonnées de texture,
//! compression `EXT_meshopt_compression` et quantification `KHR_mesh_quantization`.

use std::{borrow::Cow, path::Path};

use base64::Engine;
use rayon::prelude::*;
use serde_json::Value;

use crate::{Material, Mesh, Model, Part, ReadError, SubMesh, Texture};

fn invalid(m: impl Into<String>) -> ReadError {
    ReadError::Invalid(m.into())
}

type Mat4 = [f64; 16];

const IDENTITY: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut r = [0.0; 16];
    for c in 0..4 {
        for row in 0..4 {
            r[c * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[c * 4 + k]).sum();
        }
    }
    r
}

fn node_matrix(n: &Value) -> Mat4 {
    if let Some(m) = n.get("matrix").and_then(Value::as_array) {
        let mut out = IDENTITY;
        for (i, v) in m.iter().enumerate().take(16) {
            out[i] = v.as_f64().unwrap_or(0.0);
        }
        return out;
    }
    let get = |k: &str, d: &[f64]| -> Vec<f64> {
        n.get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect())
            .unwrap_or(d.to_vec())
    };
    let t = get("translation", &[0.0, 0.0, 0.0]);
    let q = get("rotation", &[0.0, 0.0, 0.0, 1.0]);
    let s = get("scale", &[1.0, 1.0, 1.0]);
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let r = [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + z * w),
        2.0 * (x * z - y * w),
        2.0 * (x * y - z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + x * w),
        2.0 * (x * z + y * w),
        2.0 * (y * z - x * w),
        1.0 - 2.0 * (x * x + y * y),
    ];
    [
        r[0] * s[0],
        r[1] * s[0],
        r[2] * s[0],
        0.0,
        r[3] * s[1],
        r[4] * s[1],
        r[5] * s[1],
        0.0,
        r[6] * s[2],
        r[7] * s[2],
        r[8] * s[2],
        0.0,
        t[0],
        t[1],
        t[2],
        1.0,
    ]
}

struct Doc<'a> {
    json: Value,
    buffers: Vec<Cow<'a, [u8]>>,
    /// Vues décompressées (`EXT_meshopt_compression`), par indice de vue.
    decoded: Vec<Option<Vec<u8>>>,
}

/// Décode une vue compressée par meshoptimizer.
fn decode_meshopt(ext: &Value, buffers: &[Cow<'_, [u8]>]) -> Result<Vec<u8>, ReadError> {
    let buf = buffers.get(ext["buffer"].as_u64().unwrap_or(0) as usize).ok_or_else(|| invalid("tampon compressé"))?;
    let off = ext["byteOffset"].as_u64().unwrap_or(0) as usize;
    let len = ext["byteLength"].as_u64().unwrap_or(0) as usize;
    let stride = ext["byteStride"].as_u64().unwrap_or(0) as usize;
    let count = ext["count"].as_u64().unwrap_or(0) as usize;
    let src = buf.get(off..off + len).ok_or_else(|| invalid("vue compressée hors du tampon"))?;
    let mut out = vec![0u8; stride * count];
    let rc = unsafe {
        // SAFETY: `out` a la taille `count × stride` exigée ; meshoptimizer valide l'entrée (sûr pour des données
        // non fiables, au pire des valeurs absurdes).
        match ext["mode"].as_str().unwrap_or("") {
            "ATTRIBUTES" => meshopt::ffi::meshopt_decodeVertexBuffer(
                out.as_mut_ptr().cast(),
                count,
                stride,
                src.as_ptr(),
                src.len(),
            ),
            "TRIANGLES" => {
                meshopt::ffi::meshopt_decodeIndexBuffer(out.as_mut_ptr().cast(), count, stride, src.as_ptr(), src.len())
            }
            "INDICES" => meshopt::ffi::meshopt_decodeIndexSequence(
                out.as_mut_ptr().cast(),
                count,
                stride,
                src.as_ptr(),
                src.len(),
            ),
            m => return Err(invalid(format!("mode de compression inconnu : {m}"))),
        }
    };
    if rc != 0 {
        return Err(invalid("données meshoptimizer illisibles"));
    }
    unsafe {
        // SAFETY: filtres appliqués en place sur `count` éléments de `stride` octets.
        match ext["filter"].as_str().unwrap_or("NONE") {
            "OCTAHEDRAL" => meshopt::ffi::meshopt_decodeFilterOct(out.as_mut_ptr().cast(), count, stride),
            "QUATERNION" => meshopt::ffi::meshopt_decodeFilterQuat(out.as_mut_ptr().cast(), count, stride),
            "EXPONENTIAL" => meshopt::ffi::meshopt_decodeFilterExp(out.as_mut_ptr().cast(), count, stride),
            _ => {}
        }
    }
    Ok(out)
}

impl Doc<'_> {
    /// Octets d'une vue (décompressée au besoin) et son pas.
    fn view(&self, i: usize) -> Result<(&[u8], Option<usize>), ReadError> {
        let bv = &self.json["bufferViews"][i];
        let stride = bv["byteStride"].as_u64().map(|s| s as usize);
        if let Some(Some(d)) = self.decoded.get(i) {
            return Ok((d, stride));
        }
        let buf =
            self.buffers.get(bv["buffer"].as_u64().unwrap_or(0) as usize).ok_or_else(|| invalid("tampon absent"))?;
        let off = bv["byteOffset"].as_u64().unwrap_or(0) as usize;
        let len = bv["byteLength"].as_u64().unwrap_or(0) as usize;
        Ok((buf.get(off..off + len).ok_or_else(|| invalid("vue hors du tampon"))?, stride))
    }

    /// Valeurs d'un accesseur, converties en f64, `n` composantes par élément.
    fn floats(&self, index: usize, n: usize) -> Result<Vec<f64>, ReadError> {
        let a = &self.json["accessors"][index];
        let count = a["count"].as_u64().ok_or_else(|| invalid("accesseur sans nombre"))? as usize;
        let ctype = a["componentType"].as_u64().unwrap_or(0);
        let normalized = a["normalized"].as_bool().unwrap_or(false);
        let (bin, stride) =
            self.view(a["bufferView"].as_u64().ok_or_else(|| invalid("accesseur clairsemé non géré"))? as usize)?;
        let size = match ctype {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return Err(invalid(format!("type de composante {ctype}"))),
        };
        let stride = stride.unwrap_or(size * n);
        let base = a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let end = base + stride * count.saturating_sub(1) + size * n;
        if count > 0 && end > bin.len() {
            return Err(invalid("accesseur hors de sa vue"));
        }
        let mut out = Vec::with_capacity(count * n);
        for i in 0..count {
            let p = base + i * stride;
            for k in 0..n {
                let o = p + k * size;
                let b = &bin[o..o + size];
                let v = match ctype {
                    5126 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
                    5125 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
                    5123 => {
                        let v = u16::from_le_bytes([b[0], b[1]]) as f64;
                        if normalized { v / 65535.0 } else { v }
                    }
                    5122 => {
                        let v = i16::from_le_bytes([b[0], b[1]]) as f64;
                        if normalized { (v / 32767.0).max(-1.0) } else { v }
                    }
                    5121 => {
                        let v = b[0] as f64;
                        if normalized { v / 255.0 } else { v }
                    }
                    _ => {
                        let v = b[0] as i8 as f64;
                        if normalized { (v / 127.0).max(-1.0) } else { v }
                    }
                };
                out.push(v);
            }
        }
        Ok(out)
    }

    fn primitive(&self, prim: &Value, m: &Mat4) -> Result<Mesh, ReadError> {
        if prim["mode"].as_u64().unwrap_or(4) != 4 {
            return Ok(Mesh::default());
        }
        let attrs = &prim["attributes"];
        let pi = attrs["POSITION"].as_u64().ok_or_else(|| invalid("primitive sans POSITION"))? as usize;
        let p = self.floats(pi, 3)?;
        let vcount = p.len() / 3;
        let normal_m = normal_matrix(m);
        let positions: Vec<[f32; 3]> = p
            .chunks_exact(3)
            .map(|v| {
                let x = m[0] * v[0] + m[4] * v[1] + m[8] * v[2] + m[12];
                let y = m[1] * v[0] + m[5] * v[1] + m[9] * v[2] + m[13];
                let z = m[2] * v[0] + m[6] * v[1] + m[10] * v[2] + m[14];
                [x as f32, y as f32, z as f32]
            })
            .collect();
        let indices: Vec<u32> = match prim["indices"].as_u64() {
            Some(i) => self.floats(i as usize, 1)?.into_iter().map(|v| v as u32).collect(),
            None => (0..vcount as u32).collect(),
        };
        if indices.iter().any(|&i| i as usize >= vcount) {
            return Err(invalid("indice hors des sommets"));
        }
        let uvs: Vec<[f32; 2]> = match attrs["TEXCOORD_0"].as_u64() {
            Some(ti) => self.floats(ti as usize, 2)?.chunks_exact(2).map(|t| [t[0] as f32, t[1] as f32]).collect(),
            None => Vec::new(),
        };
        let normals = match attrs["NORMAL"].as_u64() {
            Some(ni) => self
                .floats(ni as usize, 3)?
                .chunks_exact(3)
                .map(|n| {
                    let x = normal_m[0] * n[0] + normal_m[3] * n[1] + normal_m[6] * n[2];
                    let y = normal_m[1] * n[0] + normal_m[4] * n[1] + normal_m[7] * n[2];
                    let z = normal_m[2] * n[0] + normal_m[5] * n[1] + normal_m[8] * n[2];
                    let l = (x * x + y * y + z * z).sqrt().max(1e-12);
                    [(x / l) as f32, (y / l) as f32, (z / l) as f32]
                })
                .collect(),
            None => flat_normals(&positions, &indices),
        };
        // Repère inversé (déterminant négatif) : ordre des sommets retourné, faces toujours vers l'extérieur.
        let det = m[0] * (m[5] * m[10] - m[9] * m[6]) - m[4] * (m[1] * m[10] - m[9] * m[2])
            + m[8] * (m[1] * m[6] - m[5] * m[2]);
        let mut indices = indices;
        if det < 0.0 {
            for t in indices.chunks_exact_mut(3) {
                t.swap(1, 2);
            }
        }
        let material = prim["material"].as_u64().map(|v| v as u32);
        let count = indices.len() as u32;
        Ok(Mesh { positions, normals, uvs, indices, submeshes: vec![SubMesh { first: 0, count, material }] })
    }
}

/// Matrice des normales (inverse transposée de la partie 3×3, à un facteur près), par colonnes :
/// colonnes de la comatrice = produits vectoriels des colonnes de la matrice.
fn normal_matrix(m: &Mat4) -> [f64; 9] {
    let a0 = [m[0], m[1], m[2]];
    let a1 = [m[4], m[5], m[6]];
    let a2 = [m[8], m[9], m[10]];
    let cross =
        |u: [f64; 3], v: [f64; 3]| [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let (c0, c1, c2) = (cross(a1, a2), cross(a2, a0), cross(a0, a1));
    [c0[0], c0[1], c0[2], c1[0], c1[1], c1[2], c2[0], c2[1], c2[2]]
}

/// Normales par sommet moyennées par aire quand le fichier n'en donne pas.
fn flat_normals(p: &[[f32; 3]], idx: &[u32]) -> Vec<[f32; 3]> {
    let mut n = vec![[0.0f32; 3]; p.len()];
    for t in idx.chunks_exact(3) {
        let (a, b, c) = (p[t[0] as usize], p[t[1] as usize], p[t[2] as usize]);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let f = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        for &i in t {
            for k in 0..3 {
                n[i as usize][k] += f[k];
            }
        }
    }
    for v in &mut n {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-20);
        *v = [v[0] / l, v[1] / l, v[2] / l];
    }
    n
}

/// Lit un fichier `.glb` (ou `.gltf` avec tampons en `data:` ou en fichiers voisins). Chaque nœud portant un
/// maillage devient une pièce.
pub fn read_glb(path: &Path) -> Result<Model, ReadError> {
    let file = std::fs::File::open(path)?;
    // SAFETY: lecture seule ; le fichier n'est pas modifié pendant la lecture par Obview.
    let map = unsafe { memmap2::Mmap::map(&file)? };
    let data: &[u8] = &map;
    let dir = path.parent().unwrap_or(Path::new("."));
    let (json, bin): (Value, &[u8]) = if data.len() >= 20 && &data[0..4] == b"glTF" {
        let json_len = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
        if &data[16..20] != b"JSON" || 20 + json_len > data.len() {
            return Err(invalid("bloc JSON absent"));
        }
        let json: Value = serde_json::from_slice(&data[20..20 + json_len])?;
        let bin_start = 20 + json_len;
        let bin = if bin_start + 8 <= data.len() && &data[bin_start + 4..bin_start + 8] == b"BIN\0" {
            let len = u32::from_le_bytes(data[bin_start..bin_start + 4].try_into().unwrap()) as usize;
            &data[bin_start + 8..(bin_start + 8 + len).min(data.len())]
        } else {
            &[][..]
        };
        (json, bin)
    } else {
        (serde_json::from_slice(data)?, &[][..])
    };
    // Tampons : le bloc binaire, puis `data:` ou fichiers voisins ; le tampon de repli de la compression peut
    // être absent (sans `uri`).
    let mut buffers: Vec<Cow<'_, [u8]>> = Vec::new();
    for (i, b) in json["buffers"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        match b["uri"].as_str() {
            Some(uri) => buffers.push(Cow::Owned(load_uri(uri, dir)?)),
            None if i == 0 => buffers.push(Cow::Borrowed(bin)),
            None => buffers.push(Cow::Borrowed(&[])),
        }
    }
    let views = json["bufferViews"].as_array().cloned().unwrap_or_default();
    let decoded: Result<Vec<Option<Vec<u8>>>, ReadError> = views
        .par_iter()
        .map(|v| {
            match v["extensions"]
                .get("EXT_meshopt_compression")
                .or_else(|| v["extensions"].get("KHR_meshopt_compression"))
            {
                Some(ext) => decode_meshopt(ext, &buffers).map(Some),
                None => Ok(None),
            }
        })
        .collect();
    let doc = Doc { json, buffers, decoded: decoded? };
    let (materials, textures) = read_materials(&doc, dir);

    // Parcours des nœuds de la scène, transformations cumulées.
    let scene = doc.json["scene"].as_u64().unwrap_or(0) as usize;
    let roots: Vec<usize> = doc.json["scenes"][scene]["nodes"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_u64().map(|v| v as usize)).collect())
        .unwrap_or_default();
    let mut todo: Vec<(usize, Mat4)> = roots.into_iter().map(|r| (r, IDENTITY)).collect();
    let mut instances = Vec::new();
    let mut guard = 0usize;
    while let Some((ni, parent)) = todo.pop() {
        guard += 1;
        if guard > 1_000_000 {
            return Err(invalid("graphe de nœuds cyclique"));
        }
        let n = &doc.json["nodes"][ni];
        let m = mul(&parent, &node_matrix(n));
        if let Some(mi) = n["mesh"].as_u64() {
            let name =
                n["name"].as_str().or_else(|| doc.json["meshes"][mi as usize]["name"].as_str()).map(str::to_owned);
            instances.push((mi as usize, m, name));
        }
        if let Some(children) = n["children"].as_array() {
            for c in children.iter().rev() {
                if let Some(c) = c.as_u64() {
                    todo.push((c as usize, m));
                }
            }
        }
    }
    instances.sort_by_key(|(mi, _, _)| *mi);
    // Sans nom dans le fichier : numérotation dans l'ordre des maillages du fichier.
    let instances: Vec<(usize, Mat4, String)> = instances
        .into_iter()
        .enumerate()
        .map(|(k, (mi, m, name))| (mi, m, name.unwrap_or_else(|| format!("Pièce {}", k + 1))))
        .collect();

    let parts: Result<Vec<Part>, ReadError> = instances
        .par_iter()
        .map(|(mi, m, name)| {
            let prims = doc.json["meshes"][*mi]["primitives"].as_array().cloned().unwrap_or_default();
            let meshes: Result<Vec<Mesh>, ReadError> = prims.par_iter().map(|p| doc.primitive(p, m)).collect();
            let mut mesh = Mesh::default();
            for sub in meshes? {
                mesh.append(sub);
            }
            Ok(Part { name: name.clone(), mesh })
        })
        .collect();
    Ok(Model { parts: parts?, unit_m: 1.0, materials, textures })
}

/// Contenu d'une adresse de tampon ou d'image : `data:…;base64,…` ou chemin relatif au fichier.
fn load_uri(uri: &str, dir: &Path) -> Result<Vec<u8>, ReadError> {
    if let Some(rest) = uri.strip_prefix("data:") {
        let b64 = rest.split_once(";base64,").map(|(_, d)| d).ok_or_else(|| invalid("adresse data: sans base64"))?;
        return base64::engine::general_purpose::STANDARD.decode(b64).map_err(|_| invalid("base64 illisible"));
    }
    let decoded = percent_decode(uri);
    Ok(std::fs::read(dir.join(decoded))?)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Matériaux et textures. Une texture illisible est ignorée (le matériau garde ses couleurs).
fn read_materials(doc: &Doc<'_>, dir: &Path) -> (Vec<Material>, Vec<Texture>) {
    let j = &doc.json;
    // Images → textures d'Obview, dans l'ordre des images du fichier.
    let images = j["images"].as_array().cloned().unwrap_or_default();
    let mut textures = Vec::with_capacity(images.len());
    let mut image_slot: Vec<Option<u32>> = Vec::with_capacity(images.len());
    for (i, im) in images.iter().enumerate() {
        let data = if let Some(bv) = im["bufferView"].as_u64() {
            doc.view(bv as usize).map(|(d, _)| d.to_vec()).ok()
        } else if let Some(uri) = im["uri"].as_str() {
            load_uri(uri, dir).ok()
        } else {
            None
        };
        match data {
            Some(data) if !data.is_empty() => {
                let mime = im["mimeType"].as_str().unwrap_or(Texture::sniff(&data)).to_string();
                let name = im["name"].as_str().map(str::to_owned).unwrap_or_else(|| format!("image_{i}"));
                image_slot.push(Some(textures.len() as u32));
                textures.push(Texture { name, mime, data });
            }
            _ => image_slot.push(None),
        }
    }
    // Texture glTF → image (source directe ou extension WebP / Basis).
    let texture_image = |t: &Value| -> Option<u32> {
        let tex = &j["textures"][t["index"].as_u64()? as usize];
        let src = tex["source"]
            .as_u64()
            .or_else(|| tex["extensions"]["EXT_texture_webp"]["source"].as_u64())
            .or_else(|| tex["extensions"]["KHR_texture_basisu"]["source"].as_u64())?;
        image_slot.get(src as usize).copied().flatten()
    };
    let f = |v: &Value, d: f32| v.as_f64().map(|x| x as f32).unwrap_or(d);
    let materials = j["materials"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let pbr = &m["pbrMetallicRoughness"];
            let bc = &pbr["baseColorFactor"];
            let em = &m["emissiveFactor"];
            Material {
                name: m["name"].as_str().map(str::to_owned).unwrap_or_else(|| format!("Matériau {}", i + 1)),
                base_color: [f(&bc[0], 1.0), f(&bc[1], 1.0), f(&bc[2], 1.0), f(&bc[3], 1.0)],
                metallic: f(&pbr["metallicFactor"], 1.0),
                roughness: f(&pbr["roughnessFactor"], 1.0),
                emissive: [f(&em[0], 0.0), f(&em[1], 0.0), f(&em[2], 0.0)],
                base_color_texture: texture_image(&pbr["baseColorTexture"]),
                metallic_roughness_texture: texture_image(&pbr["metallicRoughnessTexture"]),
                normal_texture: texture_image(&m["normalTexture"]),
                occlusion_texture: texture_image(&m["occlusionTexture"]),
                emissive_texture: texture_image(&m["emissiveTexture"]),
                double_sided: m["doubleSided"].as_bool().unwrap_or(false),
                blend: m["alphaMode"].as_str() == Some("BLEND"),
            }
        })
        .collect();
    (materials, textures)
}
