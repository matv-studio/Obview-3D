//! Icônes vectorielles rendues au pixel exact de l'écran (resvg), mises en cache dans l'atlas.

use std::collections::HashMap;

use resvg::{tiny_skia, usvg};

use crate::{Atlas, Color, DrawList, Rect, icon_list::ICONS};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    index: u16,
    px: u16,
    stroke_x100: u16,
}

pub struct Icons {
    cache: HashMap<Key, Option<([f32; 4], u32)>>,
    names: HashMap<&'static str, u16>,
}

impl Default for Icons {
    fn default() -> Self {
        Self::new()
    }
}

impl Icons {
    pub fn new() -> Self {
        let names = ICONS.iter().enumerate().map(|(i, (n, _))| (*n, i as u16)).collect();
        Icons { cache: HashMap::new(), names }
    }

    pub fn exists(&self, name: &str) -> bool {
        self.names.contains_key(name)
    }

    /// Dessine l'icône `name` centrée dans `rect` (taille `size` en pixels logiques).
    pub fn draw(&mut self, dl: &mut DrawList, atlas: &mut Atlas, name: &str, rect: Rect, size: f32, color: Color) {
        let Some(&index) = self.names.get(name) else {
            log::warn!("icône inconnue : {name}");
            return;
        };
        let s = dl.scale;
        let px = (size * s).round().max(1.0) as u16;
        // Trait d'environ 1,4 px logique quelle que soit la taille (grille Lucide de 24).
        let stroke = (1.4 * 24.0 / size).clamp(1.2, 2.2);
        let key = Key { index, px, stroke_x100: (stroke * 100.0) as u16 };
        let entry =
            *self.cache.entry(key).or_insert_with(|| rasterize(ICONS[index as usize].1, px as u32, stroke, atlas));
        let Some((uv, side)) = entry else { return };
        let c = rect.center();
        let x0 = (c.x * s - side as f32 * 0.5).round();
        let y0 = (c.y * s - side as f32 * 0.5).round();
        dl.coverage([x0, y0, x0 + side as f32, y0 + side as f32], uv, color, 1.0);
    }
}

fn rasterize(svg: &str, px: u32, stroke: f32, atlas: &mut Atlas) -> Option<([f32; 4], u32)> {
    let svg = svg.replace("currentColor", "#000").replace("stroke-width=\"2\"", &format!("stroke-width=\"{stroke}\""));
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(px, px)?;
    let k = px as f32 / tree.size().width();
    resvg::render(&tree, tiny_skia::Transform::from_scale(k, k), &mut pixmap.as_mut());
    let alpha: Vec<u8> = pixmap.pixels().iter().map(|p| p.alpha()).collect();
    let (x, y) = atlas.insert(px, px, &alpha)?;
    Some((atlas.uv(x, y, px, px), px))
}
