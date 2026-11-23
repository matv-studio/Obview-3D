//! Arbre d'assemblage (D-35) : nœuds à transformation locale, placement de chaque pièce sur un nœud, prototype de
//! géométrie partagé entre les pièces identiques. Les pièces du modèle gardent leurs positions globales (celles
//! que l'affichage utilise) ; l'arbre sert à l'export, qui retrouve la géométrie locale de chaque prototype.

/// Matrice 4 × 4 rangée par colonnes (convention glTF).
pub type Mat4 = [f64; 16];

pub const IDENTITY: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// Nœud de l'arbre : un sous-ensemble, ou l'instance d'une pièce.
#[derive(Clone, Debug)]
pub struct Node {
    pub name: String,
    /// Parent (indice plus petit que celui du nœud), aucun pour une racine.
    pub parent: Option<u32>,
    /// Transformation relative au parent, dans les unités du modèle.
    pub local: Mat4,
}

/// Place d'une pièce dans l'arbre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    /// Nœud dont la transformation globale amène la géométrie locale du prototype aux positions de la pièce.
    pub node: u32,
    /// Pièces de même prototype : même géométrie locale et mêmes matériaux.
    pub proto: u32,
}

pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut r = [0.0; 16];
    for c in 0..4 {
        for row in 0..4 {
            r[c * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[c * 4 + k]).sum();
        }
    }
    r
}

/// Matrice depuis 3 lignes de 4 valeurs (rotation et échelle, puis translation en dernière colonne).
pub fn from_rows_3x4(m: &[f64; 12]) -> Mat4 {
    let mut r = IDENTITY;
    for row in 0..3 {
        for c in 0..4 {
            r[c * 4 + row] = m[row * 4 + c];
        }
    }
    r
}

/// Déterminant de la partie linéaire (négatif : symétrie).
pub fn det3(m: &Mat4) -> f64 {
    let a = |r: usize, c: usize| m[c * 4 + r];
    a(0, 0) * (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1)) - a(0, 1) * (a(1, 0) * a(2, 2) - a(1, 2) * a(2, 0))
        + a(0, 2) * (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0))
}

/// Inverse d'une transformation affine (dernière ligne 0 0 0 1) ; `None` si elle est dégénérée.
pub fn inverse_affine(m: &Mat4) -> Option<Mat4> {
    let a = |r: usize, c: usize| m[c * 4 + r];
    let d = det3(m);
    if d.abs() < 1e-300 {
        return None;
    }
    let inv = |r: usize, c: usize| -> f64 {
        // Cofacteur (c, r) / det.
        let (r0, r1) = ([1, 0, 0][c], [2, 2, 1][c]);
        let (c0, c1) = ([1, 0, 0][r], [2, 2, 1][r]);
        let s = if (r + c).is_multiple_of(2) { 1.0 } else { -1.0 };
        s * (a(r0, c0) * a(r1, c1) - a(r0, c1) * a(r1, c0)) / d
    };
    let mut out = IDENTITY;
    for r in 0..3 {
        for c in 0..3 {
            out[c * 4 + r] = inv(r, c);
        }
    }
    for r in 0..3 {
        out[12 + r] = -(0..3).map(|k| out[k * 4 + r] * m[12 + k]).sum::<f64>();
    }
    Some(out)
}

pub fn point(m: &Mat4, p: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
}

pub fn vector(m: &Mat4, v: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|r| m[r] * v[0] + m[4 + r] * v[1] + m[8 + r] * v[2])
}

/// Plus grand facteur d'échelle de la partie linéaire (norme des colonnes).
pub fn max_scale(m: &Mat4) -> f64 {
    (0..3).map(|c| (0..3).map(|r| m[c * 4 + r].powi(2)).sum::<f64>().sqrt()).fold(0.0, f64::max)
}

pub fn is_identity(m: &Mat4, eps: f64) -> bool {
    m.iter().zip(IDENTITY.iter()).all(|(a, b)| (a - b).abs() <= eps)
}

/// Transformations globales des nœuds (parents rangés avant leurs enfants).
pub fn world_matrices(nodes: &[Node]) -> Vec<Mat4> {
    let mut out: Vec<Mat4> = Vec::with_capacity(nodes.len());
    for n in nodes {
        let w = match n.parent.and_then(|p| out.get(p as usize)) {
            Some(p) => mul(p, &n.local),
            None => n.local,
        };
        out.push(w);
    }
    out
}

/// Chemin d'un nœud, noms des ancêtres compris, séparés par « / ».
pub fn path(nodes: &[Node], mut i: u32) -> String {
    let mut names = Vec::new();
    let mut guard = 0;
    while let Some(n) = nodes.get(i as usize) {
        if !n.name.is_empty() {
            names.push(n.name.as_str());
        }
        guard += 1;
        match n.parent {
            Some(p) if guard < 10_000 => i = p,
            _ => break,
        }
    }
    names.reverse();
    names.join(" / ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse_et_produit() {
        // Rotation de 90° autour de Z, échelle 2, translation (1, 2, 3).
        let m = from_rows_3x4(&[0.0, -2.0, 0.0, 1.0, 2.0, 0.0, 0.0, 2.0, 0.0, 0.0, 2.0, 3.0]);
        let i = inverse_affine(&m).unwrap();
        assert!(is_identity(&mul(&m, &i), 1e-12));
        let p = point(&m, [1.0, 0.0, 0.0]);
        assert!((p[0] - 1.0).abs() < 1e-12 && (p[1] - 4.0).abs() < 1e-12 && (p[2] - 3.0).abs() < 1e-12);
        assert!((det3(&m) - 8.0).abs() < 1e-12);
        assert!((max_scale(&m) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn chemins_et_globales() {
        let t = |x: f64| from_rows_3x4(&[1.0, 0.0, 0.0, x, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
        let nodes = vec![
            Node { name: "A".into(), parent: None, local: t(1.0) },
            Node { name: "B".into(), parent: Some(0), local: t(2.0) },
        ];
        let w = world_matrices(&nodes);
        assert_eq!(w[1][12], 3.0);
        assert_eq!(path(&nodes, 1), "A / B");
    }
}
