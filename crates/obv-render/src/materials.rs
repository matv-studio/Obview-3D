//! Matériaux des fichiers sur le GPU (D-37) : table de matériaux métal-rugosité lue par les nuanceurs, textures
//! (couleur de base, métal-rugosité) décodées et rangées dans un tableau de textures carrées de même taille, avec
//! leurs niveaux de détail. Les textures de couleur sont réduites en lumière linéaire (pas d'assombrissement des
//! niveaux réduits) ; les cartes de données le sont telles quelles.

use rayon::prelude::*;

/// Matériau tel que les nuanceurs le lisent (48 octets, dispositions std430 et scalaire identiques).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMaterial {
    /// Couleur de base linéaire, alpha.
    pub base: [f32; 4],
    pub emissive: [f32; 3],
    pub metallic: f32,
    pub roughness: f32,
    /// Couche de la texture de couleur, -1 : aucune.
    pub base_tex: i32,
    /// Couche de la texture métal (B) - rugosité (G), -1 : aucune.
    pub mr_tex: i32,
    pub pad: f32,
}

impl GpuMaterial {
    /// Matière par défaut (pièce sans matériau) : diélectrique satiné de la couleur donnée (sRGB 8 bits).
    pub fn satin(srgb: [u8; 4]) -> Self {
        let l = |v: u8| srgb_to_linear(v as f32 / 255.0);
        GpuMaterial {
            base: [l(srgb[0]), l(srgb[1]), l(srgb[2]), srgb[3] as f32 / 255.0],
            emissive: [0.0; 3],
            metallic: 0.0,
            roughness: 0.42,
            base_tex: -1,
            mr_tex: -1,
            pad: 0.0,
        }
    }
}

/// Image source : RGBA 8 bits, lignes du haut vers le bas ; `srgb` : couleur (réduite en linéaire).
pub struct SourceImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub srgb: bool,
}

/// Décode une image PNG, JPEG ou WebP en RGBA 8 bits.
pub fn decode(data: &[u8], srgb: bool) -> Option<SourceImage> {
    let img = image::load_from_memory(data).ok()?.to_rgba8();
    Some(SourceImage { width: img.width(), height: img.height(), rgba: img.into_raw(), srgb })
}

/// Tableau de textures prêt à envoyer : `layers[couche][niveau]`, niveaux de `size` à 1 pixel.
pub struct TextureArray {
    pub size: u32,
    pub levels: u32,
    pub layers: Vec<Vec<Vec<u8>>>,
}

impl TextureArray {
    /// Une couche blanche d'un pixel (aucune texture).
    pub fn empty() -> Self {
        TextureArray { size: 1, levels: 1, layers: vec![vec![vec![255; 4]]] }
    }

    /// Côté commun : la plus grande image (puissance de 2), 2048 au plus, réduit jusqu'à tenir dans
    /// `budget` octets (niveaux compris, un tiers de plus).
    pub fn build(images: &[SourceImage], budget: u64) -> Self {
        if images.is_empty() {
            return Self::empty();
        }
        let biggest = images.iter().map(|i| i.width.max(i.height)).max().unwrap_or(1).max(1);
        let mut size = biggest.next_power_of_two().min(2048);
        while size > 64 && (size as u64 * size as u64 * 4 * 4 / 3) * images.len() as u64 > budget {
            size /= 2;
        }
        let levels = size.ilog2() + 1;
        let layers = images.par_iter().map(|img| mip_chain(img, size, levels)).collect();
        TextureArray { size, levels, layers }
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// Image ramenée à `size` × `size` puis réduite de moitié en moitié (moyenne de 2 × 2, en linéaire pour la couleur).
fn mip_chain(img: &SourceImage, size: u32, levels: u32) -> Vec<Vec<u8>> {
    let src = image::RgbaImage::from_raw(img.width, img.height, img.rgba.clone())
        .unwrap_or_else(|| image::RgbaImage::new(1, 1));
    let top = if img.width == size && img.height == size {
        src
    } else {
        image::imageops::resize(&src, size, size, image::imageops::FilterType::CatmullRom)
    };
    let lut: Vec<f32> =
        (0..256).map(|v| if img.srgb { srgb_to_linear(v as f32 / 255.0) } else { v as f32 / 255.0 }).collect();
    let encode = |v: f32| -> u8 {
        let v = v.clamp(0.0, 1.0);
        ((if img.srgb { linear_to_srgb(v) } else { v }) * 255.0).round() as u8
    };
    let mut out = vec![top.into_raw()];
    let mut s = size;
    for _ in 1..levels {
        let prev = out.last().unwrap();
        let n = (s / 2).max(1);
        let mut next = vec![0u8; (n * n * 4) as usize];
        for y in 0..n {
            for x in 0..n {
                for c in 0..4 {
                    let at = |xx: u32, yy: u32| prev[((yy.min(s - 1) * s + xx.min(s - 1)) * 4 + c) as usize];
                    let (a, b, d, e) =
                        (at(2 * x, 2 * y), at(2 * x + 1, 2 * y), at(2 * x, 2 * y + 1), at(2 * x + 1, 2 * y + 1));
                    // Alpha et données : moyenne directe ; couleur : moyenne en linéaire.
                    let v = if c == 3 {
                        (a as f32 + b as f32 + d as f32 + e as f32) / (4.0 * 255.0)
                    } else {
                        (lut[a as usize] + lut[b as usize] + lut[d as usize] + lut[e as usize]) / 4.0
                    };
                    next[((y * n + x) * 4 + c) as usize] = if c == 3 { (v * 255.0).round() as u8 } else { encode(v) };
                }
            }
        }
        out.push(next);
        s = n;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn niveaux_et_moyenne_lineaire() {
        // Damier noir et blanc 4 × 4 : la réduction en linéaire donne 50 % de lumière, soit 188 en sRGB (et non
        // 128, gris trop sombre d'une moyenne des valeurs codées).
        let mut rgba = Vec::new();
        for y in 0..4u32 {
            for x in 0..4u32 {
                let v = if (x + y) % 2 == 0 { 255 } else { 0 };
                rgba.extend([v, v, v, 255]);
            }
        }
        let img = SourceImage { width: 4, height: 4, rgba, srgb: true };
        let t = TextureArray::build(&[img], u64::MAX);
        assert_eq!((t.size, t.levels), (4, 3));
        let l1 = &t.layers[0][1];
        assert_eq!(l1.len(), 2 * 2 * 4);
        assert!((l1[0] as i32 - 188).abs() <= 1, "{}", l1[0]);
        assert_eq!(t.layers[0][2].len(), 4);
    }

    #[test]
    fn budget_reduit_la_taille() {
        let img = || SourceImage { width: 1024, height: 512, rgba: vec![128; 1024 * 512 * 4], srgb: false };
        let t = TextureArray::build(&[img(), img()], 4 * 1024 * 1024);
        assert!(t.size <= 512 && t.size >= 64, "{}", t.size);
        assert_eq!(t.layers[0][0].len() as u32, t.size * t.size * 4);
    }
}
