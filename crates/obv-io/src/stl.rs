//! STL binaire et texte. Le fichier est projeté en mémoire ; les triangles binaires sont décodés en parallèle,
//! puis soudés et lissés par angle de pli (les STL n'ont qu'une normale par face).

use std::path::Path;

use rayon::prelude::*;

use crate::{Mesh, Model, Part, ReadError, normals};

/// Angle de pli des normales lissées (même valeur que les arêtes vives de l'affichage).
pub const CREASE_DEG: f32 = 30.0;

pub fn read_stl(path: &Path) -> Result<Model, ReadError> {
    let file = std::fs::File::open(path)?;
    // SAFETY: le fichier est seulement lu ; une modification extérieure pendant la lecture donnerait des
    // valeurs fausses, pas une faute mémoire de notre part (projection en lecture seule).
    let map = unsafe { memmap2::Mmap::map(&file)? };
    let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mesh = parse_stl(&map)?;
    Ok(Model { parts: vec![Part { name, mesh }], unit_m: 0.001, ..Default::default() })
}

/// Décode un STL depuis la mémoire (binaire si la taille correspond, texte sinon).
pub fn parse_stl(data: &[u8]) -> Result<Mesh, ReadError> {
    let binary_len =
        |d: &[u8]| (d.len() >= 84).then(|| 84 + 50 * u32::from_le_bytes(d[80..84].try_into().unwrap()) as usize);
    // Un STL binaire peut commencer par « solid » : la taille annoncée fait foi.
    let corners = if binary_len(data) == Some(data.len()) || !data.trim_ascii_start().starts_with(b"solid") {
        let Some(len) = binary_len(data) else { return Err(ReadError::Invalid("STL trop court".into())) };
        if data.len() < len {
            return Err(ReadError::Invalid("STL binaire tronqué".into()));
        }
        let count = (len - 84) / 50;
        let body = &data[84..len];
        let mut corners = vec![[0f32; 3]; count * 3];
        corners.par_chunks_mut(3).zip(body.par_chunks(50)).for_each(|(out, tri)| {
            for (k, c) in out.iter_mut().enumerate() {
                let off = 12 + k * 12;
                *c = std::array::from_fn(|i| f32::from_le_bytes(tri[off + i * 4..off + i * 4 + 4].try_into().unwrap()));
            }
        });
        corners
    } else {
        parse_ascii(data)?
    };
    if corners.iter().any(|c| c.iter().any(|v| !v.is_finite())) {
        return Err(ReadError::Invalid("coordonnées non finies".into()));
    }
    Ok(normals::from_triangle_soup(&corners, CREASE_DEG))
}

fn parse_ascii(data: &[u8]) -> Result<Vec<[f32; 3]>, ReadError> {
    let text = std::str::from_utf8(data).map_err(|_| ReadError::Invalid("STL texte non UTF-8".into()))?;
    let mut corners = Vec::new();
    for line in text.lines() {
        let mut it = line.split_ascii_whitespace();
        if it.next() == Some("vertex") {
            let mut v = [0f32; 3];
            for c in &mut v {
                *c = it
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| ReadError::Invalid(format!("sommet illisible : {line}")))?;
            }
            corners.push(v);
        }
    }
    if corners.len() % 3 != 0 {
        return Err(ReadError::Invalid("STL texte : nombre de sommets non multiple de 3".into()));
    }
    Ok(corners)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tetra_corners() -> Vec<[f32; 3]> {
        let p = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]].iter().flat_map(|t| t.map(|i| p[i])).collect()
    }

    #[test]
    fn binaire_et_texte_identiques() {
        let corners = tetra_corners();
        let mut bin = vec![0u8; 80];
        bin[..5].copy_from_slice(b"solid"); // en-tête trompeur, comme certains exports
        bin.extend((corners.len() as u32 / 3).to_le_bytes());
        for t in corners.chunks(3) {
            bin.extend([0u8; 12]);
            for c in t {
                for v in c {
                    bin.extend(v.to_le_bytes());
                }
            }
            bin.extend([0u8; 2]);
        }
        let mut txt = String::from("solid t\n");
        for t in corners.chunks(3) {
            txt += " facet normal 0 0 0\n  outer loop\n";
            for c in t {
                txt += &format!("   vertex {} {} {}\n", c[0], c[1], c[2]);
            }
            txt += "  endloop\n endfacet\n";
        }
        txt += "endsolid t\n";
        let a = parse_stl(&bin).unwrap();
        let b = parse_stl(txt.as_bytes()).unwrap();
        assert_eq!(a.triangle_count(), 4);
        assert_eq!(a.positions, b.positions);
        assert_eq!(a.indices, b.indices);
        // Tétraèdre : toutes les arêtes sont vives (angles dièdres ≈ 70° et 90°), 3 normales par sommet.
        assert_eq!(a.positions.len(), 12);
    }
}
