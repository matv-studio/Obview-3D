//! Matériaux du modèle prêts pour le GPU (D-37) : table (matériau 0 : acier satiné des pièces sans matériau,
//! puis ceux du fichier) et tableau de textures décodées, préparés hors du fil de l'interface.

use obv_io::Model;
use obv_render::materials::{GpuMaterial, SourceImage, TextureArray, decode};

/// Mémoire au plus pour le tableau de textures (niveaux compris).
const TEXTURE_BUDGET: u64 = 512 * 1024 * 1024;

pub struct Prepared {
    pub table: Vec<GpuMaterial>,
    pub textures: TextureArray,
}

impl Default for Prepared {
    fn default() -> Self {
        Prepared { table: vec![GpuMaterial::satin(crate::frame::STEEL)], textures: TextureArray::empty() }
    }
}

/// Indice dans la table d'un matériau du fichier (0 : aucun).
pub fn index(material: Option<u32>) -> u32 {
    material.map(|m| m + 1).unwrap_or(0)
}

pub fn prepare(model: &Model) -> Prepared {
    // Couleur (sRGB, réduite en linéaire) : textures de couleur de base et d'émission ; le reste : données.
    let srgb: Vec<bool> = (0..model.textures.len() as u32)
        .map(|t| model.materials.iter().any(|m| m.base_color_texture == Some(t) || m.emissive_texture == Some(t)))
        .collect();
    let images: Vec<SourceImage> = model
        .textures
        .iter()
        .zip(&srgb)
        .map(|(t, &s)| decode(&t.data, s).unwrap_or(SourceImage { width: 1, height: 1, rgba: vec![255; 4], srgb: s }))
        .collect();
    let textures = TextureArray::build(&images, TEXTURE_BUDGET);
    let layer = |t: Option<u32>| t.filter(|t| (*t as usize) < model.textures.len()).map(|t| t as i32).unwrap_or(-1);
    let mut table = vec![GpuMaterial::satin(crate::frame::STEEL)];
    table.extend(model.materials.iter().map(|m| GpuMaterial {
        base: m.base_color,
        emissive: m.emissive,
        metallic: m.metallic,
        roughness: m.roughness,
        base_tex: layer(m.base_color_texture),
        mr_tex: layer(m.metallic_roughness_texture),
        pad: 0.0,
    }));
    Prepared { table, textures }
}

#[cfg(test)]
mod tests {
    use super::*;
    use obv_io::{Material, Texture};

    #[test]
    fn table_et_couches() {
        let mut png = Vec::new();
        {
            let mut e = png::Encoder::new(&mut png, 2, 2);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            e.write_header().unwrap().write_image_data(&[200; 16]).unwrap();
        }
        let model = Model {
            materials: vec![Material {
                metallic: 1.0,
                roughness: 0.2,
                base_color_texture: Some(0),
                ..Default::default()
            }],
            textures: vec![Texture { name: "t".into(), mime: "image/png".into(), data: png }],
            ..Default::default()
        };
        let p = prepare(&model);
        assert_eq!(p.table.len(), 2);
        assert_eq!((p.table[1].base_tex, p.table[1].mr_tex), (0, -1));
        assert_eq!(p.textures.layers.len(), 1);
        assert_eq!(index(None), 0);
        assert_eq!(index(Some(0)), 1);
    }
}
