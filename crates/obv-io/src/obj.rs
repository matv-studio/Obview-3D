//! Wavefront OBJ et MTL, lecture parallèle : le fichier projeté en mémoire est coupé en tranches aux fins de ligne,
//! chaque tranche est décodée sur son propre cœur, puis les indices relatifs sont résolus avec les décalages
//! cumulés. Les groupes `o` / `g` deviennent des pièces ; `usemtl` découpe chaque pièce en plages par matériau ;
//! les polygones sont découpés en éventail ; sans normales dans le fichier, elles sont lissées par angle de pli.
//! Les matériaux MTL (couleurs, PBR `Pr`/`Pm`, cartes) et leurs images sont lus à côté du fichier.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use rayon::prelude::*;

use crate::{Material, Mesh, Model, Part, ReadError, SubMesh, Texture, normals, srgb_to_linear, stl::CREASE_DEG};

/// Indice brut d'un coin : absolu (à partir de 0) ou relatif au début de la tranche.
#[derive(Clone, Copy, Default)]
struct Ref {
    idx: i64,
    rel: bool,
}

#[derive(Clone, Copy, Default)]
struct Corner {
    v: Ref,
    vt: Option<Ref>,
    vn: Option<Ref>,
}

/// Coin résolu : sommet, coordonnée de texture, normale.
type ObjCorner = (u32, Option<u32>, Option<u32>);

#[derive(Default)]
struct Chunk {
    v: Vec<[f32; 3]>,
    vt: Vec<[f32; 2]>,
    vn: Vec<[f32; 3]>,
    corners: Vec<Corner>,
    /// Nombre de coins de chaque polygone.
    sizes: Vec<u32>,
    /// Changements de groupe et de matériau : (indice du polygone dans la tranche, nom).
    groups: Vec<(usize, String)>,
    materials: Vec<(usize, String)>,
    mtllibs: Vec<String>,
}

fn parse_floats<const N: usize>(it: &mut std::str::SplitAsciiWhitespace<'_>) -> Option<[f32; N]> {
    let mut out = [0f32; N];
    for o in &mut out {
        *o = it.next()?.parse::<f32>().ok()?;
    }
    Some(out)
}

fn parse_ref(s: &str, local_count: usize) -> Option<Ref> {
    if s.is_empty() {
        return None;
    }
    let i: i64 = s.parse().ok()?;
    match i {
        0 => None,
        i if i > 0 => Some(Ref { idx: i - 1, rel: false }),
        i => Some(Ref { idx: local_count as i64 + i, rel: true }),
    }
}

fn parse_chunk(text: &str) -> Result<Chunk, ReadError> {
    let mut c = Chunk::default();
    for line in text.lines() {
        let line = line.trim_start();
        let mut it = line.split_ascii_whitespace();
        match it.next() {
            Some("v") => c.v.push(parse_floats::<3>(&mut it).ok_or_else(|| bad(line))?),
            Some("vn") => c.vn.push(parse_floats::<3>(&mut it).ok_or_else(|| bad(line))?),
            Some("vt") => {
                let u = it.next().and_then(|s| s.parse::<f32>().ok()).ok_or_else(|| bad(line))?;
                let v = it.next().and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
                c.vt.push([u, v]);
            }
            Some("f") => {
                let start = c.corners.len();
                for tok in it {
                    let mut parts = tok.split('/');
                    let v = parse_ref(parts.next().unwrap_or(""), c.v.len()).ok_or_else(|| bad(line))?;
                    let vt = parts.next().and_then(|s| parse_ref(s, c.vt.len()));
                    let vn = parts.next().and_then(|s| parse_ref(s, c.vn.len()));
                    c.corners.push(Corner { v, vt, vn });
                }
                let n = c.corners.len() - start;
                if n < 3 {
                    c.corners.truncate(start);
                } else {
                    c.sizes.push(n as u32);
                }
            }
            Some("o") | Some("g") => c.groups.push((c.sizes.len(), line[1..].trim().to_string())),
            Some("usemtl") => c.materials.push((c.sizes.len(), line[6..].trim().to_string())),
            Some("mtllib") => c.mtllibs.push(line[6..].trim().to_string()),
            _ => {}
        }
    }
    Ok(c)
}

fn bad(line: &str) -> ReadError {
    ReadError::Invalid(format!("OBJ : ligne illisible : {}", line.chars().take(80).collect::<String>()))
}

/// Tranches de taille voisine, coupées après une fin de ligne.
fn split(text: &str, pieces: usize) -> Vec<&str> {
    let bytes = text.as_bytes();
    let target = (bytes.len() / pieces.max(1)).max(1 << 20);
    let mut out = Vec::new();
    let mut start = 0;
    while start < bytes.len() {
        let mut end = (start + target).min(bytes.len());
        while end < bytes.len() && bytes[end - 1] != b'\n' {
            end += 1;
        }
        out.push(&text[start..end]);
        start = end;
    }
    out
}

pub fn read_obj(path: &Path) -> Result<Model, ReadError> {
    let file = std::fs::File::open(path)?;
    // SAFETY: projection en lecture seule (voir `read_stl`).
    let map = unsafe { memmap2::Mmap::map(&file)? };
    let text = std::str::from_utf8(&map).map_err(|_| ReadError::Invalid("OBJ non UTF-8".into()))?;
    let fallback = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    parse_obj_in(text, &fallback, path.parent())
}

/// Décode un OBJ sans fichiers voisins (pas de MTL).
pub fn parse_obj(text: &str, fallback_name: &str) -> Result<Model, ReadError> {
    parse_obj_in(text, fallback_name, None)
}

/// Lit un fichier MTL : matériaux, et les images qu'ils citent (ajoutées à `textures`).
fn read_mtl(path: &Path, textures: &mut Vec<Texture>, loaded: &mut HashMap<PathBuf, u32>) -> Vec<Material> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut out: Vec<Material> = Vec::new();
    let mut texture = |rest: &str, textures: &mut Vec<Texture>| -> Option<u32> {
        // Les options (`-bm 1`, `-o u v`…) précèdent le nom du fichier : on garde le dernier élément.
        let file = rest.split_ascii_whitespace().last()?;
        let p = dir.join(file.replace('\\', "/"));
        if let Some(i) = loaded.get(&p) {
            return Some(*i);
        }
        let data = std::fs::read(&p).ok()?;
        let i = textures.len() as u32;
        textures.push(Texture {
            name: p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            mime: Texture::sniff(&data).to_string(),
            data,
        });
        loaded.insert(p, i);
        Some(i)
    };
    for line in text.lines() {
        let line = line.trim();
        let (key, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let rest = rest.trim();
        let nums = || rest.split_ascii_whitespace().filter_map(|s| s.parse::<f32>().ok()).collect::<Vec<_>>();
        if key == "newmtl" {
            out.push(Material { name: rest.to_string(), roughness: 0.6, ..Default::default() });
            continue;
        }
        let Some(m) = out.last_mut() else { continue };
        match key {
            "Kd" => {
                let v = nums();
                if v.len() >= 3 {
                    m.base_color[..3].copy_from_slice(&[
                        srgb_to_linear(v[0]),
                        srgb_to_linear(v[1]),
                        srgb_to_linear(v[2]),
                    ]);
                }
            }
            "d" => m.base_color[3] = nums().first().copied().unwrap_or(1.0),
            "Tr" => m.base_color[3] = 1.0 - nums().first().copied().unwrap_or(0.0),
            // Brillance Phong → rugosité (approximation usuelle).
            "Ns" => m.roughness = (2.0 / (nums().first().copied().unwrap_or(10.0) + 2.0)).sqrt().clamp(0.0, 1.0),
            "Pr" => m.roughness = nums().first().copied().unwrap_or(m.roughness),
            "Pm" => m.metallic = nums().first().copied().unwrap_or(0.0),
            "Ke" => {
                let v = nums();
                if v.len() >= 3 {
                    m.emissive = [v[0], v[1], v[2]];
                }
            }
            "map_Kd" => m.base_color_texture = texture(rest, textures),
            "map_Bump" | "map_bump" | "bump" | "norm" | "map_Kn" => m.normal_texture = texture(rest, textures),
            "map_Ke" => m.emissive_texture = texture(rest, textures),
            "map_Pr" | "map_Pm" | "map_RMA" | "map_ORM" => {
                m.metallic_roughness_texture = m.metallic_roughness_texture.or(texture(rest, textures))
            }
            _ => {}
        }
        m.blend = m.base_color[3] < 0.999;
    }
    out
}

fn parse_obj_in(text: &str, fallback_name: &str, dir: Option<&Path>) -> Result<Model, ReadError> {
    let chunks: Vec<Chunk> =
        split(text, rayon::current_num_threads() * 4).into_par_iter().map(parse_chunk).collect::<Result<_, _>>()?;
    // Matériaux des bibliothèques citées.
    let mut materials: Vec<Material> = Vec::new();
    let mut textures: Vec<Texture> = Vec::new();
    let mut loaded = HashMap::new();
    if let Some(dir) = dir {
        for lib in chunks.iter().flat_map(|c| &c.mtllibs) {
            materials.extend(read_mtl(&dir.join(lib), &mut textures, &mut loaded));
        }
    }
    let mut mat_index: HashMap<String, u32> =
        materials.iter().enumerate().map(|(i, m)| (m.name.clone(), i as u32)).collect();

    // Décalages cumulés.
    let (mut vb, mut tb, mut nb) = (Vec::new(), Vec::new(), Vec::new());
    let (mut tv, mut tt, mut tn) = (0i64, 0i64, 0i64);
    for c in &chunks {
        vb.push(tv);
        tb.push(tt);
        nb.push(tn);
        tv += c.v.len() as i64;
        tt += c.vt.len() as i64;
        tn += c.vn.len() as i64;
    }
    let v: Vec<[f32; 3]> = chunks.iter().flat_map(|c| c.v.iter().copied()).collect();
    let vt: Vec<[f32; 2]> = chunks.iter().flat_map(|c| c.vt.iter().copied()).collect();
    let vn: Vec<[f32; 3]> = chunks.iter().flat_map(|c| c.vn.iter().copied()).collect();
    let resolve = |r: Ref, base: i64, len: usize| -> Result<u32, ReadError> {
        let i = if r.rel { base + r.idx } else { r.idx };
        if i < 0 || i as usize >= len {
            return Err(ReadError::Invalid(format!("OBJ : indice hors limites ({})", i + 1)));
        }
        Ok(i as u32)
    };

    // Polygones regroupés par pièce, dans l'ordre d'apparition des noms.
    #[derive(Default)]
    struct Group {
        name: String,
        corners: Vec<ObjCorner>,
        sizes: Vec<u32>,
        face_material: Vec<Option<u32>>,
    }
    let mut groups: Vec<Group> = vec![Group::default()];
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let mut current = 0usize;
    let mut material: Option<u32> = None;
    for (ci, c) in chunks.iter().enumerate() {
        let (mut gi, mut mi, mut k) = (0, 0, 0usize);
        let mut switch_to = |name: &str, groups: &mut Vec<Group>| {
            *by_name.entry(name.to_string()).or_insert_with(|| {
                groups.push(Group { name: name.to_string(), ..Default::default() });
                groups.len() - 1
            })
        };
        for (fi, &n) in c.sizes.iter().enumerate() {
            while gi < c.groups.len() && c.groups[gi].0 == fi {
                current = switch_to(&c.groups[gi].1, &mut groups);
                gi += 1;
            }
            while mi < c.materials.len() && c.materials[mi].0 == fi {
                let name = &c.materials[mi].1;
                material = Some(*mat_index.entry(name.clone()).or_insert_with(|| {
                    materials.push(Material { name: name.clone(), roughness: 0.6, ..Default::default() });
                    (materials.len() - 1) as u32
                }));
                mi += 1;
            }
            let g = &mut groups[current];
            for corner in &c.corners[k..k + n as usize] {
                let vi = resolve(corner.v, vb[ci], v.len())?;
                let ti = corner.vt.map(|r| resolve(r, tb[ci], vt.len())).transpose()?;
                let ni = corner.vn.map(|r| resolve(r, nb[ci], vn.len())).transpose()?;
                g.corners.push((vi, ti, ni));
            }
            g.sizes.push(n);
            g.face_material.push(material);
            k += n as usize;
        }
        while gi < c.groups.len() {
            current = switch_to(&c.groups[gi].1, &mut groups);
            gi += 1;
        }
        while mi < c.materials.len() {
            let name = &c.materials[mi].1;
            material = Some(*mat_index.entry(name.clone()).or_insert_with(|| {
                materials.push(Material { name: name.clone(), roughness: 0.6, ..Default::default() });
                (materials.len() - 1) as u32
            }));
            mi += 1;
        }
    }

    groups.retain(|g| !g.sizes.is_empty());
    let parts: Vec<Part> = groups
        .into_par_iter()
        .enumerate()
        .map(|(i, g)| {
            // Éventail : (c0, ck, ck+1), triangles rangés par matériau (ordre stable).
            let mut tris: Vec<([ObjCorner; 3], Option<u32>)> = Vec::new();
            let mut k = 0;
            for (f, &n) in g.sizes.iter().enumerate() {
                let poly = &g.corners[k..k + n as usize];
                for j in 1..n as usize - 1 {
                    tris.push(([poly[0], poly[j], poly[j + 1]], g.face_material[f]));
                }
                k += n as usize;
            }
            tris.sort_by_key(|t| t.1.map(|m| m as i64).unwrap_or(-1));
            let corners: Vec<ObjCorner> = tris.iter().flat_map(|t| t.0).collect();
            let has_uv = corners.iter().all(|c| c.1.is_some()) && !vt.is_empty();
            // Normales : celles du fichier, ou lissées par angle de pli (positions soudées par indice).
            let normals: Vec<[f32; 3]> = if corners.iter().all(|c| c.2.is_some()) {
                corners.iter().map(|c| vn[c.2.unwrap() as usize]).collect()
            } else {
                let pos_idx: Vec<u32> = corners.iter().map(|c| c.0).collect();
                normals::corner_normals(&v, &pos_idx, CREASE_DEG)
            };
            let mut m = Mesh::default();
            let mut map: HashMap<(u32, u32, [u32; 3]), u32> = HashMap::with_capacity(corners.len());
            for (c, n) in corners.iter().zip(&normals) {
                let key = (c.0, if has_uv { c.1.unwrap() } else { u32::MAX }, normals::vec_key(*n));
                let idx = *map.entry(key).or_insert_with(|| {
                    m.positions.push(v[c.0 as usize]);
                    m.normals.push(*n);
                    if has_uv {
                        m.uvs.push(vt[c.1.unwrap() as usize]);
                    }
                    (m.positions.len() - 1) as u32
                });
                m.indices.push(idx);
            }
            // Plages par matériau.
            let mut first = 0u32;
            for chunk in tris.chunk_by(|a, b| a.1 == b.1) {
                let count = chunk.len() as u32 * 3;
                m.submeshes.push(SubMesh { first, count, material: chunk[0].1 });
                first += count;
            }
            if m.submeshes.iter().all(|s| s.material.is_none()) {
                m.submeshes.clear();
            }
            let name = if g.name.is_empty() { format!("{fallback_name} {}", i + 1) } else { g.name };
            Part { name, mesh: m }
        })
        .collect();
    Ok(Model { parts, unit_m: 0.001, materials, textures })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUBE: &str = "\
# cube avec deux groupes, un quadrilatère, indices négatifs
o Haut
v 0 0 0
v 1 0 0
v 1 1 0
v 0 1 0
f 1 2 3 4
o Bas
v 0 0 1
v 1 0 1
v 1 1 1
vn 0 0 1
f -3//1 -2//1 -1//1
o Haut
f 1 3 4
";

    #[test]
    fn groupes_polygones_et_indices_relatifs() {
        let m = parse_obj(CUBE, "cube").unwrap();
        assert_eq!(m.parts.len(), 2);
        assert_eq!(m.parts[0].name, "Haut");
        assert_eq!(m.parts[0].mesh.triangle_count(), 3);
        assert_eq!(m.parts[1].name, "Bas");
        assert_eq!(m.parts[1].mesh.triangle_count(), 1);
        assert_eq!(m.parts[1].mesh.positions[0], [0.0, 0.0, 1.0]);
        assert_eq!(m.parts[1].mesh.normals[0], [0.0, 0.0, 1.0]);
    }

    #[test]
    fn materiaux_textures_uv() {
        let dir = std::env::temp_dir().join("obv-io-obj-mtl");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.png"), [0x89, b'P', b'N', b'G', 1, 2, 3]).unwrap();
        std::fs::write(dir.join("a.mtl"), "newmtl Rouge\nKd 1 0 0\nmap_Kd -bm 1 a.png\nnewmtl Bleu\nKd 0 0 1\nd 0.5\n")
            .unwrap();
        let obj = "mtllib a.mtl\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n\
                   usemtl Bleu\nf 1/1 2/2 3/3\nusemtl Rouge\nf 1/1 3/3 4/4\n";
        std::fs::write(dir.join("a.obj"), obj).unwrap();
        let m = read_obj(&dir.join("a.obj")).unwrap();
        assert_eq!(m.materials.len(), 2);
        assert_eq!(m.textures.len(), 1);
        assert_eq!(m.textures[0].mime, "image/png");
        assert_eq!(m.materials[0].base_color_texture, Some(0));
        assert!(m.materials[1].blend);
        let mesh = &m.parts[0].mesh;
        assert_eq!(mesh.uvs.len(), mesh.positions.len());
        assert_eq!(mesh.submeshes.len(), 2);
        // Rangées par matériau : Rouge (0) avant Bleu (1).
        assert_eq!(mesh.submeshes[0].material, Some(0));
    }

    #[test]
    fn tranches_paralleles_identiques() {
        let mut s = String::new();
        for i in 0..60_000 {
            s += &format!("v {i} 0 0\nv {i} 1 0\nv {i} 0 1\nf -3 -2 -1\n");
        }
        let m = parse_obj(&s, "x").unwrap();
        assert_eq!(m.triangle_count(), 60_000);
        assert!(split(&s, 8).len() > 1);
    }

    #[test]
    fn indice_hors_limites() {
        assert!(parse_obj("v 0 0 0\nf 1 2 3\n", "x").is_err());
    }
}
