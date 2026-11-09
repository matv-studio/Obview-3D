//! Contexte d'interface : entrées, survol, clics, info-bulles, animations, et passage d'une image.

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

use crate::{Atlas, Color, DrawList, Fonts, Icons, Node, Rect, TextStyle, Theme, Tooltip, Vec2, Weight, vec2};

/// Identifiant stable d'un élément interactif.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Id(pub u64);

impl Id {
    pub fn new(name: &str) -> Id {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        name.hash(&mut h);
        Id(h.finish())
    }
    /// Identifiant dérivé. Les entiers de tout type donnent le même résultat (`with(3)` = `with(3usize)`).
    pub fn with(self, key: impl IdPart) -> Id {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.0.hash(&mut h);
        key.part().hash(&mut h);
        Id(h.finish())
    }
}

/// Clé d'un identifiant dérivé.
pub trait IdPart {
    fn part(&self) -> (u8, u64);
}

impl IdPart for &str {
    fn part(&self) -> (u8, u64) {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut h);
        (1, h.finish())
    }
}

macro_rules! int_part {
    ($($t:ty),*) => {$(
        impl IdPart for $t {
            fn part(&self) -> (u8, u64) {
                (0, *self as u64)
            }
        }
    )*};
}
int_part!(u8, u16, u32, u64, usize, i32, i64);

/// Ressources partagées pendant la mesure et le dessin.
pub struct Resources<'a> {
    pub fonts: &'a mut Fonts,
    pub icons: &'a mut Icons,
    pub atlas: &'a mut Atlas,
    pub scale: f32,
}

pub struct Painter<'a> {
    pub dl: &'a mut DrawList,
    pub res: Resources<'a>,
    pub theme: &'a Theme,
}

/// Entrées d'une image, en pixels logiques.
#[derive(Clone, Debug, Default)]
pub struct FrameInput {
    /// Taille de la fenêtre en pixels logiques.
    pub size: Vec2,
    pub scale: f32,
    /// Temps écoulé depuis le lancement, en secondes.
    pub time: f64,
    pub pointer: Option<Vec2>,
    pub primary_down: bool,
    pub scroll: Vec2,
}

pub struct Ui {
    pub theme: Theme,
    pub fonts: Fonts,
    pub icons: Icons,
    pub atlas: Atlas,
    pub dl: DrawList,
    pub input: FrameInput,
    prev_down: bool,
    hits: Vec<(Id, Rect, Option<Tooltip>)>,
    hovered: Option<Id>,
    active: Option<Id>,
    clicked: Option<Id>,
    hover_since: f64,
    anims: HashMap<Id, f32>,
    animating: bool,
    dt: f32,
    last_time: f64,
    /// Montre l'info-bulle de cet élément sans attendre (captures, essais).
    pub force_tooltip: Option<Id>,
}

const TOOLTIP_DELAY: f64 = 0.55;

impl Ui {
    pub fn new(theme: Theme) -> Self {
        Ui {
            theme,
            fonts: Fonts::new(),
            icons: Icons::new(),
            atlas: Atlas::new(2048),
            dl: DrawList::new(1.0),
            input: FrameInput { scale: 1.0, ..Default::default() },
            prev_down: false,
            hits: Vec::new(),
            hovered: None,
            active: None,
            clicked: None,
            hover_since: 0.0,
            anims: HashMap::new(),
            animating: false,
            dt: 0.0,
            last_time: 0.0,
            force_tooltip: None,
        }
    }

    /// Début d'une image : met à jour survol et clics d'après la mise en page de l'image précédente.
    pub fn begin_frame(&mut self, input: FrameInput) {
        self.dt = ((input.time - self.last_time) as f32).clamp(0.0, 0.1);
        self.last_time = input.time;
        let pressed = input.primary_down && !self.prev_down;
        let released = !input.primary_down && self.prev_down;
        self.prev_down = input.primary_down;
        let hovered =
            input.pointer.and_then(|p| self.hits.iter().rev().find(|(_, r, _)| r.contains(p)).map(|(id, _, _)| *id));
        if hovered != self.hovered {
            self.hover_since = input.time;
        }
        self.hovered = hovered;
        self.clicked = None;
        if pressed {
            self.active = hovered;
        }
        if released {
            if self.active.is_some() && self.active == hovered {
                self.clicked = self.active;
            }
            self.active = None;
        }
        self.input = input;
        self.animating = false;
    }

    pub fn hovered(&self, id: Id) -> bool {
        self.hovered == Some(id) && (self.active.is_none() || self.active == Some(id))
    }
    pub fn pressed(&self, id: Id) -> bool {
        self.active == Some(id) && self.hovered == Some(id)
    }
    pub fn clicked(&self, id: Id) -> bool {
        self.clicked == Some(id)
    }
    /// Rectangle de l'élément `id` à la dernière mise en page.
    pub fn rect_of(&self, id: Id) -> Option<Rect> {
        self.hits.iter().rev().find(|(h, _, _)| *h == id).map(|(_, r, _)| *r)
    }

    /// Un élément est-il survolé (pour savoir si la vue 3D reçoit la souris) ?
    pub fn pointer_over_ui(&self) -> bool {
        self.hovered.is_some()
    }

    /// Valeur animée vers `target` (lissage exponentiel, ~`ms` millisecondes pour 95 %).
    pub fn anim(&mut self, id: Id, target: f32, ms: f32) -> f32 {
        let k = if ms <= 0.0 { 1.0 } else { 1.0 - (-3.0 * self.dt * 1000.0 / ms).exp() };
        let v = self.anims.entry(id).or_insert(target);
        *v += (target - *v) * k;
        if (target - *v).abs() < 0.002 {
            *v = target;
        } else {
            self.animating = true;
        }
        *v
    }

    /// Une nouvelle image est-elle nécessaire sans nouvelle entrée (animation, info-bulle en attente) ?
    pub fn wants_redraw(&self) -> bool {
        self.animating
            || (self.hovered.is_some()
                && self.input.time - self.hover_since < TOOLTIP_DELAY + 0.05
                && self.hit_tooltip(self.hovered.unwrap()).is_some())
    }

    fn hit_tooltip(&self, id: Id) -> Option<(&Rect, &Tooltip)> {
        self.hits.iter().rev().find(|(h, _, _)| *h == id).and_then(|(_, r, t)| t.as_ref().map(|t| (r, t)))
    }

    /// Fin d'une image : place l'arbre dans la fenêtre, dessine, puis les info-bulles par-dessus.
    pub fn finish(&mut self, root: &mut Node) {
        let scale = self.input.scale.max(0.5);
        self.dl.clear(scale);
        let window = Rect { min: Vec2::default(), max: self.input.size };
        {
            let mut res = Resources { fonts: &mut self.fonts, icons: &mut self.icons, atlas: &mut self.atlas, scale };
            root.layout(window, &mut res);
        }
        self.hits.clear();
        root.collect_hits(window, &mut self.hits);
        {
            let mut p = Painter {
                dl: &mut self.dl,
                res: Resources { fonts: &mut self.fonts, icons: &mut self.icons, atlas: &mut self.atlas, scale },
                theme: &self.theme,
            };
            root.paint(&mut p);
        }
        let tip_id = self.force_tooltip.or_else(|| {
            let h = self.hovered?;
            (self.active.is_none() && self.input.time - self.hover_since >= TOOLTIP_DELAY).then_some(h)
        });
        if let Some(id) = tip_id
            && let Some((r, t)) = self.hit_tooltip(id).map(|(r, t)| (*r, t.clone()))
        {
            self.paint_tooltip(r, &t, window);
        }
    }

    fn paint_tooltip(&mut self, anchor: Rect, tip: &Tooltip, window: Rect) {
        let c = &self.theme.color;
        let scale = self.dl.scale;
        let style = TextStyle::new(12.0, Weight::Medium);
        let small = TextStyle::new(11.0, Weight::Medium);
        let text = self.fonts.layout(&tip.text, style, scale);
        let key = tip.shortcut.as_ref().map(|s| self.fonts.layout(s, small, scale));
        let pad = vec2(8.0, 5.0);
        let key_w = key.as_ref().map(|k| k.width + 10.0 + 8.0).unwrap_or(0.0);
        let w = (text.width + key_w + pad.x * 2.0).ceil();
        let h = 26.0;
        let mut x = (anchor.center().x - w * 0.5).round();
        x = x.clamp(window.min.x + 6.0, window.max.x - w - 6.0);
        let mut y = anchor.max.y + 6.0;
        if y + h > window.max.y - 6.0 {
            y = anchor.min.y - 6.0 - h;
        }
        let r = Rect::from_xywh(x, y, w, h);
        self.dl.shadow(r, 6.0, 10.0, vec2(0.0, 3.0), c.shadow);
        self.dl.rect(r, 6.0, c.tooltip_bg, None);
        self.fonts.draw(
            &mut self.dl,
            &mut self.atlas,
            &text,
            vec2(x + pad.x, y + ((h - text.height) * 0.5).round()),
            c.tooltip_text,
        );
        if let Some(k) = key {
            let kx = x + pad.x + text.width + 8.0;
            let kr = Rect::from_xywh(kx, y + 5.0, k.width + 10.0, h - 10.0);
            self.dl.rect(kr, 4.0, c.tooltip_text.with_alpha(0.12), None);
            self.fonts.draw(
                &mut self.dl,
                &mut self.atlas,
                &k,
                vec2(kx + 5.0, kr.min.y + ((kr.height() - k.height) * 0.5).round()),
                c.tooltip_text.with_alpha(0.75),
            );
        }
    }

    /// Couleur d'un fond selon l'état (repos, survol, pression), avec transition douce.
    pub fn state_color(&mut self, id: Id, rest: Color, hover: Color, press: Color) -> Color {
        let h = self.anim(id.with("survol"), if self.hovered(id) { 1.0 } else { 0.0 }, 90.0);
        let p = if self.pressed(id) { 1.0 } else { 0.0 };
        rest.mix(hover, h).mix(press, p)
    }
}
