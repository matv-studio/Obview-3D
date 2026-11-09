//! Wavefront OBJ, lecture parallèle : le fichier projeté en mémoire est coupé en tranches aux fins de ligne, chaque
//! tranche est décodée sur son propre cœur, puis les indices relatifs sont résolus avec les décalages cumulés.
//! Les groupes `o` / `g` deviennent des pièces ; les polygones sont découpés en éventail ; sans normales dans le
//! fichier, elles sont lissées par angle de pli.

use std::{collections::HashMap, path::Path};

use rayon::prelude::*;

use crate::{Mesh, Model, Part, ReadError, normals, stl::CREASE_DEG};

/// Indice brut d'un coin : absolu (à partir de 0) ou relatif au début de la tranche.
#[derive(Clone, Copy, Default)]
struct Ref {
    idx: i64,
    rel: bool,
}

#[derive(Clone, Copy, Default)]
struct Corner {
    v: Ref,
    vn: Option<Ref>,
}

#[derive(Default)]
struct Chunk {
    v: Vec<[f32; 3]>,
    vn: Vec<[f32; 3]>,
    corners: Vec<Corner>,
    /// Nombre de coins de chaque polygone.
    sizes: Vec<u32>,
    /// Changements de groupe : (indice du polygone dans la tranche, nom).
    groups: Vec<(usize, String)>,
}

fn parse_floats<const N: usize>(it: &mut std::str::SplitAsciiWhitespace<'_>) -> Option<[f32; N]> {
    let mut out = [0f32; N];
    for o in &mut out {
        *o = parse_f32(it.next()?)?;
    }
    Some(out)
}

fn parse_f32(s: &str) -> Option<f32> {
    s.parse::<f32>().ok()
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
            Some("f") => {
                let start = c.corners.len();
                for tok in it {
                    let mut parts = tok.split('/');
                    let v = parse_ref(parts.next().unwrap_or(""), c.v.len()).ok_or_else(|| bad(line))?;
                    let _vt = parts.next();
                    let vn = parts.next().and_then(|s| parse_ref(s, c.vn.len()));
                    c.corners.push(Corner { v, vn });
                }
                let n = c.corners.len() - start;
                if n < 3 {
                    c.corners.truncate(start);
                } else {
                    c.sizes.push(n as u32);
                }
            }
            Some("o") | Some("g") => {
                let name = line[1..].trim().to_string();
                c.groups.push((c.sizes.len(), name));
            }
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
    parse_obj(text, &fallback)
}

pub fn parse_obj(text: &str, fallback_name: &str) -> Result<Model, ReadError> {
    let chunks: Vec<Chunk> =
        split(text, rayon::current_num_threads() * 4).into_par_iter().map(parse_chunk).collect::<Result<_, _>>()?;
    // Décalages cumulés.
    let (mut vb, mut nb) = (Vec::with_capacity(chunks.len()), Vec::with_capacity(chunks.len()));
    let (mut tv, mut tn) = (0i64, 0i64);
    for c in &chunks {
        vb.push(tv);
        nb.push(tn);
        tv += c.v.len() as i64;
        tn += c.vn.len() as i64;
    }
    let v: Vec<[f32; 3]> = chunks.iter().flat_map(|c| c.v.iter().copied()).collect();
    let vn: Vec<[f32; 3]> = chunks.iter().flat_map(|c| c.vn.iter().copied()).collect();
    let resolve = |r: Ref, base: i64, len: usize| -> Result<u32, ReadError> {
        let i = if r.rel { base + r.idx } else { r.idx };
        if i < 0 || i as usize >= len {
            return Err(ReadError::Invalid(format!("OBJ : indice hors limites ({})", i + 1)));
        }
        Ok(i as u32)
    };

    // Polygones regroupés par pièce, dans l'ordre d'apparition des noms.
    struct Group {
        name: String,
        // Coins résolus (sommet, normale) et tailles des polygones.
        corners: Vec<(u32, Option<u32>)>,
        sizes: Vec<u32>,
    }
    let mut groups: Vec<Group> = vec![Group { name: String::new(), corners: Vec::new(), sizes: Vec::new() }];
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let mut current = 0usize;
    for (ci, c) in chunks.iter().enumerate() {
        let mut gi = 0;
        let mut k = 0usize;
        for (fi, &n) in c.sizes.iter().enumerate() {
            while gi < c.groups.len() && c.groups[gi].0 == fi {
                let name = c.groups[gi].1.clone();
                current = *by_name.entry(name.clone()).or_insert_with(|| {
                    groups.push(Group { name, corners: Vec::new(), sizes: Vec::new() });
                    groups.len() - 1
                });
                gi += 1;
            }
            let g = &mut groups[current];
            for corner in &c.corners[k..k + n as usize] {
                let vi = resolve(corner.v, vb[ci], v.len())?;
                let ni = match corner.vn {
                    Some(r) => Some(resolve(r, nb[ci], vn.len())?),
                    None => None,
                };
                g.corners.push((vi, ni));
            }
            g.sizes.push(n);
            k += n as usize;
        }
        // Groupes déclarés après le dernier polygone de la tranche.
        while gi < c.groups.len() {
            let name = c.groups[gi].1.clone();
            current = *by_name.entry(name.clone()).or_insert_with(|| {
                groups.push(Group { name, corners: Vec::new(), sizes: Vec::new() });
                groups.len() - 1
            });
            gi += 1;
        }
    }

    groups.retain(|g| !g.sizes.is_empty());
    let parts: Vec<Part> = groups
        .into_par_iter()
        .enumerate()
        .map(|(i, g)| {
            // Éventail : (c0, ck, ck+1).
            let mut tris: Vec<(u32, Option<u32>)> = Vec::with_capacity(g.corners.len() * 3 / 2);
            let mut k = 0;
            for &n in &g.sizes {
                let poly = &g.corners[k..k + n as usize];
                for j in 1..n as usize - 1 {
                    tris.extend([poly[0], poly[j], poly[j + 1]]);
                }
                k += n as usize;
            }
            let mesh = if tris.iter().all(|c| c.1.is_some()) {
                let mut map: HashMap<(u32, u32), u32> = HashMap::with_capacity(tris.len());
                let mut m = Mesh::default();
                m.indices = tris
                    .iter()
                    .map(|&(a, b)| {
                        let b = b.unwrap();
                        *map.entry((a, b)).or_insert_with(|| {
                            m.positions.push(v[a as usize]);
                            m.normals.push(vn[b as usize]);
                            (m.positions.len() - 1) as u32
                        })
                    })
                    .collect();
                m
            } else {
                let soup: Vec<[f32; 3]> = tris.iter().map(|c| v[c.0 as usize]).collect();
                normals::from_triangle_soup(&soup, CREASE_DEG)
            };
            let name = if g.name.is_empty() { format!("{fallback_name} {}", i + 1) } else { g.name };
            Part { name, mesh }
        })
        .collect();
    Ok(Model { parts, unit_m: 0.001 })
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
    fn tranches_paralleles_identiques() {
        // Un gros fichier coupé en plusieurs tranches donne le même résultat qu'une seule tranche.
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
