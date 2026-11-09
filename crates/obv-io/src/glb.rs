//! glTF 2.0 binaire : en-tête, bloc JSON, bloc binaire ; nœuds et transformations, accesseurs avec pas.

use std::path::Path;

use rayon::prelude::*;
use serde_json::Value;

use crate::{Mesh, Model, Part, ReadError};

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
    bin: &'a [u8],
}

impl Doc<'_> {
    /// Valeurs d'un accesseur, converties en f64, `n` composantes par élément.
    fn floats(&self, index: usize, n: usize) -> Result<Vec<f64>, ReadError> {
        let a = &self.json["accessors"][index];
        let count = a["count"].as_u64().ok_or_else(|| invalid("accesseur sans nombre"))? as usize;
        let ctype = a["componentType"].as_u64().unwrap_or(0);
        let normalized = a["normalized"].as_bool().unwrap_or(false);
        let bv = &self.json["bufferViews"]
            [a["bufferView"].as_u64().ok_or_else(|| invalid("accesseur clairsemé non géré"))? as usize];
        let size = match ctype {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return Err(invalid(format!("type de composante {ctype}"))),
        };
        let stride = bv["byteStride"].as_u64().map(|s| s as usize).unwrap_or(size * n);
        let base = bv["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let end = base + stride * count.saturating_sub(1) + size * n;
        if end > self.bin.len() {
            return Err(invalid("accesseur hors du bloc binaire"));
        }
        let mut out = Vec::with_capacity(count * n);
        for i in 0..count {
            let p = base + i * stride;
            for k in 0..n {
                let o = p + k * size;
                let b = &self.bin[o..o + size];
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
        Ok(Mesh { positions, normals, indices })
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

/// Lit un fichier `.glb`. Chaque nœud portant un maillage devient une pièce.
pub fn read_glb(path: &Path) -> Result<Model, ReadError> {
    let file = std::fs::File::open(path)?;
    // SAFETY: lecture seule ; le fichier n'est pas modifié pendant la lecture par Obview.
    let map = unsafe { memmap2::Mmap::map(&file)? };
    let data: &[u8] = &map;
    if data.len() < 20 || &data[0..4] != b"glTF" {
        return Err(invalid("en-tête glTF absent"));
    }
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
    let doc = Doc { json, bin };

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
                let base = mesh.positions.len() as u32;
                mesh.positions.extend(sub.positions);
                mesh.normals.extend(sub.normals);
                mesh.indices.extend(sub.indices.into_iter().map(|i| i + base));
            }
            Ok(Part { name: name.clone(), mesh })
        })
        .collect();
    Ok(Model { parts: parts?, unit_m: 1.0 })
}
