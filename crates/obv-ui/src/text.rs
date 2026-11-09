//! Texte : mise en forme (swash), rendu des glyphes au pixel de l'écran avec position sous-pixel, cache.

use std::collections::HashMap;

use swash::{
    FontRef,
    scale::{Render, ScaleContext, Source, image::Content},
    shape::ShapeContext,
    zeno::{Format, Vector},
};

use crate::{Atlas, Color, DrawList, Vec2};

static REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
static MEDIUM: &[u8] = include_bytes!("../assets/fonts/Inter-Medium.ttf");
static SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Weight {
    Regular,
    Medium,
    SemiBold,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub weight: Weight,
}

impl TextStyle {
    pub fn new(size: f32, weight: Weight) -> Self {
        TextStyle { size, weight }
    }
}

#[derive(Clone, Copy, Debug)]
struct PlacedGlyph {
    font: u8,
    id: u16,
    /// Position du point d'origine, en pixels de l'écran, relative au début de la ligne de base.
    x: f32,
    y: f32,
}

/// Ligne de texte mise en forme, mesurée en pixels logiques.
#[derive(Clone, Debug)]
pub struct TextLayout {
    glyphs: Vec<PlacedGlyph>,
    px: f32,
    scale: f32,
    pub width: f32,
    pub height: f32,
    /// Distance du haut de la ligne à la ligne de base.
    pub baseline: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    font: u8,
    id: u16,
    px64: u32,
    sub: u8,
}

#[derive(Clone, Copy)]
struct CachedGlyph {
    uv: [f32; 4],
    left: f32,
    top: f32,
    w: f32,
    h: f32,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct LayoutKey {
    text: String,
    size_bits: u32,
    weight: Weight,
    scale_bits: u32,
}

pub struct Fonts {
    shape: ShapeContext,
    scale_ctx: ScaleContext,
    glyphs: HashMap<GlyphKey, Option<CachedGlyph>>,
    layouts: HashMap<LayoutKey, TextLayout>,
}

const SUBPIXEL: f32 = 4.0;

fn font(i: u8) -> FontRef<'static> {
    let data = match i {
        0 => REGULAR,
        1 => MEDIUM,
        _ => SEMIBOLD,
    };
    FontRef::from_index(data, 0).expect("police Inter intégrée illisible")
}

fn font_index(w: Weight) -> u8 {
    match w {
        Weight::Regular => 0,
        Weight::Medium => 1,
        Weight::SemiBold => 2,
    }
}

impl Default for Fonts {
    fn default() -> Self {
        Self::new()
    }
}

impl Fonts {
    pub fn new() -> Self {
        Fonts {
            shape: ShapeContext::new(),
            scale_ctx: ScaleContext::new(),
            glyphs: HashMap::new(),
            layouts: HashMap::new(),
        }
    }

    /// Hauteur de ligne de l'interface pour une taille donnée.
    pub fn line_height(size: f32) -> f32 {
        (size * 1.38).round()
    }

    /// Met en forme une ligne (sans retour à la ligne). Résultat mis en cache.
    pub fn layout(&mut self, text: &str, style: TextStyle, scale: f32) -> TextLayout {
        let key = LayoutKey {
            text: text.to_owned(),
            size_bits: style.size.to_bits(),
            weight: style.weight,
            scale_bits: scale.to_bits(),
        };
        if let Some(l) = self.layouts.get(&key) {
            return l.clone();
        }
        if self.layouts.len() > 4096 {
            self.layouts.clear();
        }
        let fi = font_index(style.weight);
        let f = font(fi);
        let px = style.size * scale;
        let metrics = f.metrics(&[]).scale(px);
        let mut shaper = self.shape.builder(f).size(px).build();
        shaper.add_str(text);
        let mut glyphs = Vec::new();
        let mut x = 0.0f32;
        shaper.shape_with(|cluster| {
            for g in cluster.glyphs {
                glyphs.push(PlacedGlyph { font: fi, id: g.id, x: x + g.x, y: -g.y });
                x += g.advance;
            }
        });
        let line = Self::line_height(style.size);
        let content = (metrics.ascent + metrics.descent) / scale;
        let baseline = ((line - content) * 0.5 + metrics.ascent / scale).round();
        let layout = TextLayout { glyphs, px, scale, width: x / scale, height: line, baseline };
        self.layouts.insert(key, layout.clone());
        layout
    }

    /// Met en forme en coupant la fin par « … » si la largeur dépasse `max_width`.
    pub fn layout_truncated(&mut self, text: &str, style: TextStyle, scale: f32, max_width: f32) -> TextLayout {
        let full = self.layout(text, style, scale);
        if full.width <= max_width || text.is_empty() {
            return full;
        }
        let chars: Vec<char> = text.chars().collect();
        let (mut lo, mut hi) = (0usize, chars.len());
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let s: String = chars[..mid].iter().collect::<String>().trim_end().to_owned() + "…";
            if self.layout(&s, style, scale).width <= max_width { lo = mid } else { hi = mid - 1 }
        }
        let s: String = chars[..lo].iter().collect::<String>().trim_end().to_owned() + "…";
        self.layout(&s, style, scale)
    }

    fn glyph(&mut self, atlas: &mut Atlas, key: GlyphKey) -> Option<CachedGlyph> {
        if let Some(g) = self.glyphs.get(&key) {
            return *g;
        }
        let f = font(key.font);
        let mut scaler = self.scale_ctx.builder(f).size(key.px64 as f32 / 64.0).hint(false).build();
        let image = Render::new(&[Source::Outline])
            .format(Format::Alpha)
            .offset(Vector::new(key.sub as f32 / SUBPIXEL, 0.0))
            .render(&mut scaler, key.id);
        let cached = image.and_then(|img| {
            if img.content != Content::Mask || img.placement.width == 0 || img.placement.height == 0 {
                return None;
            }
            let (w, h) = (img.placement.width, img.placement.height);
            let (x, y) = atlas.insert(w, h, &img.data)?;
            Some(CachedGlyph {
                uv: atlas.uv(x, y, w, h),
                left: img.placement.left as f32,
                top: img.placement.top as f32,
                w: w as f32,
                h: h as f32,
            })
        });
        self.glyphs.insert(key, cached);
        cached
    }

    /// Dessine `layout` avec le coin haut-gauche de la ligne en `pos` (pixels logiques).
    pub fn draw(&mut self, dl: &mut DrawList, atlas: &mut Atlas, layout: &TextLayout, pos: Vec2, color: Color) {
        let s = layout.scale;
        let origin_x = pos.x * s;
        let baseline = (pos.y * s + layout.baseline * s).round();
        // Le texte sombre sur fond clair paraît plus maigre en mélange linéaire : on épaissit sa couverture.
        let gamma = if color.luminance() < 0.25 { 0.72 } else { 0.9 };
        let px64 = (layout.px * 64.0).round() as u32;
        for g in &layout.glyphs {
            let gx = origin_x + g.x;
            let fx = gx.floor();
            let sub = (((gx - fx) * SUBPIXEL).round() as u32 % SUBPIXEL as u32) as u8;
            let carry = if ((gx - fx) * SUBPIXEL).round() >= SUBPIXEL { 1.0 } else { 0.0 };
            let key = GlyphKey { font: g.font, id: g.id, px64, sub };
            if let Some(c) = self.glyph(atlas, key) {
                let x0 = fx + carry + c.left;
                let y0 = baseline + g.y - c.top;
                dl.coverage([x0, y0, x0 + c.w, y0 + c.h], c.uv, color, gamma);
            }
        }
    }
}
