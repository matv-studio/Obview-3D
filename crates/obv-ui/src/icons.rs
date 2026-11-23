//! Icônes vectorielles (grille 16 × 16) rendues au pixel exact de l'écran (resvg), mises en cache dans l'atlas.

use std::collections::HashMap;

use resvg::{tiny_skia, usvg};

use crate::{Atlas, Color, DrawList, Rect, icon_list::ICONS};

/// Images en couleurs (logo d'Obview, D-36), rendues au pixel exact comme les icônes.
pub(crate) static IMAGES: &[(&str, &str)] = &[
    ("obview-clair", include_str!("../assets/images/obview-clair.svg")),
    ("obview-sombre", include_str!("../assets/images/obview-sombre.svg")),
];

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    index: u16,
    px: u16,
    stroke_x100: u16,
}

/// Image en couleur rangée dans l'atlas : uv, décalage d'un plan à l'autre, côté en pixels.
type Placed = Option<([f32; 4], f32, u32)>;

pub struct Icons {
    cache: HashMap<Key, Option<([f32; 4], u32)>>,
    names: HashMap<&'static str, u16>,
    images: HashMap<(u16, u16), Placed>,
}

impl Default for Icons {
    fn default() -> Self {
        Self::new()
    }
}

impl Icons {
    pub fn new() -> Self {
        let names = ICONS.iter().enumerate().map(|(i, (n, _))| (*n, i as u16)).collect();
        Icons { cache: HashMap::new(), names, images: HashMap::new() }
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
        // Trait de 1,5 px logique quelle que soit la taille (grille de 16, charte des apps).
        let stroke = (1.5 * 16.0 / size).clamp(1.2, 2.0);
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

impl Icons {
    /// Dessine l'image en couleurs `name` centrée dans `rect`, côté `size` en pixels logiques.
    pub fn draw_image(
        &mut self,
        dl: &mut DrawList,
        atlas: &mut Atlas,
        name: &str,
        rect: Rect,
        size: f32,
        opacity: f32,
    ) {
        let Some(index) = IMAGES.iter().position(|(n, _)| *n == name) else {
            log::warn!("image inconnue : {name}");
            return;
        };
        let s = dl.scale;
        let px = (size * s).round().max(1.0) as u16;
        let entry =
            *self.images.entry((index as u16, px)).or_insert_with(|| rasterize_rgba(IMAGES[index].1, px as u32, atlas));
        let Some((uv, dx, side)) = entry else { return };
        let c = rect.center();
        let x0 = (c.x * s - side as f32 * 0.5).round();
        let y0 = (c.y * s - side as f32 * 0.5).round();
        dl.image([x0, y0, x0 + side as f32, y0 + side as f32], uv, dx, opacity);
    }
}

/// Image en couleurs rangée en quatre plans 8 bits (R, V, B non prémultipliés en sRGB, alpha), séparés d'un
/// pixel vide. Renvoie les coordonnées du premier plan, l'écart entre plans et le côté en pixels.
fn rasterize_rgba(svg: &str, px: u32, atlas: &mut Atlas) -> Option<([f32; 4], f32, u32)> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(px, px)?;
    let k = px as f32 / tree.size().width().max(tree.size().height());
    resvg::render(&tree, tiny_skia::Transform::from_scale(k, k), &mut pixmap.as_mut());
    let w = px * 4 + 3;
    let mut planes = vec![0u8; (w * px) as usize];
    for (i, p) in pixmap.pixels().iter().enumerate() {
        let c = p.demultiply();
        let (x, y) = (i as u32 % px, i as u32 / px);
        for (k, v) in [c.red(), c.green(), c.blue(), c.alpha()].into_iter().enumerate() {
            planes[(y * w + k as u32 * (px + 1) + x) as usize] = v;
        }
    }
    let (x, y) = atlas.insert(w, px, &planes)?;
    Some((atlas.uv(x, y, px, px), (px + 1) as f32 / atlas.size as f32, px))
}

fn rasterize(svg: &str, px: u32, stroke: f32, atlas: &mut Atlas) -> Option<([f32; 4], u32)> {
    let svg =
        svg.replace("currentColor", "#000").replace("stroke-width=\"1.5\"", &format!("stroke-width=\"{stroke}\""));
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(px, px)?;
    let k = px as f32 / tree.size().width();
    resvg::render(&tree, tiny_skia::Transform::from_scale(k, k), &mut pixmap.as_mut());
    let alpha: Vec<u8> = pixmap.pixels().iter().map(|p| p.alpha()).collect();
    let (x, y) = atlas.insert(px, px, &alpha)?;
    Some((atlas.uv(x, y, px, px), px))
}
