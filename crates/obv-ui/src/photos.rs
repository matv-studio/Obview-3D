//! Images en couleurs de l'interface (vignettes des fichiers, D-41) : couches d'un tableau de textures RGBA
//! sRGB de taille fixe, avec leurs niveaux de détail, attribuées et libérées par l'application. Les envois au
//! GPU attendent dans `pending` jusqu'à la préparation de l'image suivante.

use std::sync::Arc;

/// Taille d'une couche (16:10) et nombre de couches.
pub const PHOTO_W: u32 = 512;
pub const PHOTO_H: u32 = 320;
pub const PHOTO_LAYERS: u32 = 48;
/// Niveaux de détail : 512 × 320 jusqu'à 32 × 20.
pub const PHOTO_MIPS: u32 = 5;

/// Couche d'une image dans le tableau.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PhotoId(pub u32);

/// Niveaux d'une image à envoyer (RGBA8 sRGB, du plus grand au plus petit).
pub type Mips = Arc<Vec<Vec<u8>>>;

#[derive(Default)]
pub struct Photos {
    used: Vec<bool>,
    /// Images à envoyer : couche, niveaux.
    pub pending: Vec<(PhotoId, Mips)>,
}

impl Photos {
    /// Range une image `PHOTO_W` × `PHOTO_H` (RGBA8 sRGB, alpha droit) dans une couche libre.
    /// `None` : plus de couche libre (l'application libère les images qu'elle ne montre plus).
    pub fn insert(&mut self, rgba: &[u8]) -> Option<PhotoId> {
        if self.used.is_empty() {
            self.used = vec![false; PHOTO_LAYERS as usize];
        }
        let k = self.used.iter().position(|u| !u)?;
        self.used[k] = true;
        let id = PhotoId(k as u32);
        self.replace(id, rgba);
        Some(id)
    }

    /// Remplace le contenu d'une couche déjà attribuée.
    pub fn replace(&mut self, id: PhotoId, rgba: &[u8]) {
        if rgba.len() != (PHOTO_W * PHOTO_H * 4) as usize {
            log::warn!("vignette de taille inattendue : {} octets", rgba.len());
            return;
        }
        self.pending.retain(|(p, _)| *p != id);
        self.pending.push((id, Arc::new(mips(rgba))));
    }

    pub fn remove(&mut self, id: PhotoId) {
        if let Some(u) = self.used.get_mut(id.0 as usize) {
            *u = false;
        }
        self.pending.retain(|(p, _)| *p != id);
    }

    /// Couches attribuées (essais).
    pub fn len(&self) -> usize {
        self.used.iter().filter(|u| **u).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u8
}

/// Niveaux de détail par moyenne 2 × 2 en lumière linéaire, pondérée par l'alpha (bords nets sur fond
/// transparent).
pub fn mips(rgba: &[u8]) -> Vec<Vec<u8>> {
    let mut levels = vec![rgba.to_vec()];
    let (mut w, mut h) = (PHOTO_W as usize, PHOTO_H as usize);
    for _ in 1..PHOTO_MIPS {
        let src = levels.last().expect("niveau");
        let (nw, nh) = (w / 2, h / 2);
        let mut dst = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                let mut acc = [0.0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let i = ((y * 2 + dy) * w + x * 2 + dx) * 4;
                    let a = src[i + 3] as f32 / 255.0;
                    for k in 0..3 {
                        acc[k] += to_linear(src[i + k]) * a;
                    }
                    acc[3] += a;
                }
                let o = (y * nw + x) * 4;
                let a = acc[3];
                for k in 0..3 {
                    dst[o + k] = to_srgb(if a > 0.0 { acc[k] / a } else { 0.0 });
                }
                dst[o + 3] = (a / 4.0 * 255.0 + 0.5) as u8;
            }
        }
        levels.push(dst);
        (w, h) = (nw, nh);
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn couches_et_niveaux() {
        let mut p = Photos::default();
        let img = vec![255u8; (PHOTO_W * PHOTO_H * 4) as usize];
        let a = p.insert(&img).unwrap();
        let b = p.insert(&img).unwrap();
        assert_ne!(a, b);
        p.remove(a);
        assert_eq!(p.insert(&img), Some(a));
        let (_, m) = &p.pending[0];
        assert_eq!(m.len(), PHOTO_MIPS as usize);
        assert_eq!(m[PHOTO_MIPS as usize - 1].len(), 32 * 20 * 4);
        assert!(m[1].iter().all(|v| *v == 255));
    }
}
