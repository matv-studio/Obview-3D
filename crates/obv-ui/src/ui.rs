//! Contexte d'interface : entrées, survol, clics, glisser, défilement, focus et saisie, calques (menus,
//! fenêtres modales), info-bulles, animations, et passage d'une image.

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

use crate::{
    Atlas, Color, DrawList, Fonts, FrameInput, Icons, Key, Node, Rect, TextStyle, Theme, Tooltip, Vec2, Weight, vec2,
};

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

/// Forme du pointeur demandée par l'élément survolé.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cursor {
    #[default]
    Default,
    Pointer,
    Text,
    Grab,
    Grabbing,
    ResizeHorizontal,
    ResizeVertical,
}

/// Zone sensible relevée à la mise en page.
#[derive(Clone, Debug)]
pub struct Hit {
    pub id: Id,
    pub rect: Rect,
    pub tooltip: Option<Tooltip>,
    pub cursor: Cursor,
    pub layer: u8,
    pub scroll: bool,
}

/// État d'une zone défilante (pixels logiques).
#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollState {
    pub offset: f32,
    pub target: f32,
    pub content: f32,
    pub view: f32,
}

impl ScrollState {
    pub fn max(&self) -> f32 {
        (self.content - self.view).max(0.0)
    }
}

/// Curseur et sélection d'un champ de saisie, en indices de caractères.
#[derive(Clone, Copy, Debug, Default)]
pub struct EditState {
    pub caret: usize,
    pub anchor: usize,
}

/// Ressources partagées pendant la mesure et le dessin.
pub struct Resources<'a> {
    pub fonts: &'a mut Fonts,
    pub icons: &'a mut Icons,
    pub atlas: &'a mut Atlas,
    pub scale: f32,
    pub scrolls: &'a mut HashMap<Id, ScrollState>,
}

pub struct Painter<'a> {
    pub dl: &'a mut DrawList,
    pub res: Resources<'a>,
    pub theme: &'a Theme,
    pub hovered: Option<Id>,
    pub active: Option<Id>,
    pub time: f64,
}

/// Nature d'un calque posé au-dessus de l'interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    /// Menu déroulant, liste de choix : se ferme au clic extérieur ou sur Échap.
    Popup,
    /// Fenêtre modale : voile, et l'interface en dessous ne reçoit plus rien.
    Modal,
}

struct Overlay {
    node: Node,
    kind: Layer,
}

pub struct Ui {
    pub theme: Theme,
    pub fonts: Fonts,
    pub icons: Icons,
    pub atlas: Atlas,
    pub dl: DrawList,
    pub input: FrameInput,
    prev_buttons: [bool; 3],
    prev_pointer: Option<Vec2>,
    hits: Vec<Hit>,
    hovered: Option<Id>,
    active: Option<Id>,
    clicked: Option<Id>,
    double_clicked: Option<Id>,
    last_click: Option<(Id, f64)>,
    press_pos: Option<Vec2>,
    hover_since: f64,
    anims: HashMap<Id, f32>,
    animating: bool,
    dt: f32,
    last_time: f64,
    pub scrolls: HashMap<Id, ScrollState>,
    focus: Option<Id>,
    edits: HashMap<Id, EditState>,
    focus_since: f64,
    overlays: Vec<Overlay>,
    open_popup: Option<Id>,
    closed_popup: Option<Id>,
    /// Calque de la fenêtre modale la plus haute (les calques inférieurs sont inertes).
    modal_layer: Option<u8>,
    popup_layers: Vec<u8>,
    escape_used: bool,
    press_started: bool,
    /// Forme du pointeur à appliquer à la fenêtre.
    pub cursor: Cursor,
    /// Montre l'info-bulle de cet élément sans attendre (captures, essais).
    pub force_tooltip: Option<Id>,
}

const TOOLTIP_DELAY: f64 = 0.55;
const DOUBLE_CLICK: f64 = 0.35;

impl Ui {
    pub fn new(theme: Theme) -> Self {
        Ui {
            theme,
            fonts: Fonts::new(),
            icons: Icons::new(),
            atlas: Atlas::new(2048),
            dl: DrawList::new(1.0),
            input: FrameInput { scale: 1.0, ..Default::default() },
            prev_buttons: [false; 3],
            prev_pointer: None,
            hits: Vec::new(),
            hovered: None,
            active: None,
            clicked: None,
            double_clicked: None,
            last_click: None,
            press_pos: None,
            hover_since: 0.0,
            anims: HashMap::new(),
            animating: false,
            dt: 0.0,
            last_time: 0.0,
            scrolls: HashMap::new(),
            focus: None,
            edits: HashMap::new(),
            focus_since: 0.0,
            overlays: Vec::new(),
            open_popup: None,
            closed_popup: None,
            modal_layer: None,
            popup_layers: Vec::new(),
            escape_used: false,
            press_started: false,
            cursor: Cursor::Default,
            force_tooltip: None,
        }
    }

    /// Début d'une image : met à jour survol, clics, défilement d'après la mise en page de l'image précédente.
    pub fn begin_frame(&mut self, input: FrameInput) {
        self.dt = ((input.time - self.last_time) as f32).clamp(0.0, 0.1);
        self.last_time = input.time;
        let pressed = input.buttons[0] && !self.prev_buttons[0];
        let released = !input.buttons[0] && self.prev_buttons[0];
        let any_pressed = (0..3).any(|i| input.buttons[i] && !self.prev_buttons[i]);
        self.prev_buttons = input.buttons;
        self.prev_pointer = self.input.pointer;
        let min_layer = self.modal_layer.unwrap_or(0);
        let hovered = input.pointer.and_then(|p| {
            self.hits.iter().rev().find(|h| h.layer >= min_layer && !h.scroll && h.rect.contains(p)).map(|h| h.id)
        });
        if hovered != self.hovered {
            self.hover_since = input.time;
        }
        self.hovered = hovered;
        self.clicked = None;
        self.double_clicked = None;
        self.animating = false;

        // Un clic hors du menu ouvert le ferme (et le clic sur son bouton ne le rouvre pas).
        if any_pressed {
            self.closed_popup = None;
        }
        if any_pressed && let Some(p) = self.open_popup {
            let in_popup = input.pointer.is_some_and(|pt| {
                self.hits.iter().any(|h| self.popup_layers.contains(&h.layer) && h.rect.contains(pt))
            });
            if !in_popup {
                self.closed_popup = Some(p);
                self.open_popup = None;
            }
        }
        let consumed = any_pressed && self.closed_popup.is_some();
        self.press_started = pressed && !consumed;
        if pressed && !consumed {
            self.active = hovered;
            self.press_pos = input.pointer;
            // Un clic ailleurs retire le focus du champ de saisie.
            if self.focus.is_some() && hovered != self.focus {
                self.focus = None;
            }
        }
        if released {
            if self.active.is_some() && self.active == hovered {
                self.clicked = self.active;
                if let Some((id, t)) = self.last_click
                    && Some(id) == self.clicked
                    && input.time - t < DOUBLE_CLICK
                {
                    self.double_clicked = self.clicked;
                    self.last_click = None;
                } else {
                    self.last_click = self.clicked.map(|c| (c, input.time));
                }
            }
            self.active = None;
            self.press_pos = None;
        }
        self.escape_used = false;
        if input.keys.contains(&Key::Escape) {
            if self.open_popup.is_some() {
                self.closed_popup = self.open_popup.take();
                self.escape_used = true;
            } else if self.focus.is_some() {
                self.focus = None;
                self.escape_used = true;
            }
        }

        // Molette : la zone défilante la plus profonde sous le pointeur.
        if input.scroll.y != 0.0
            && let Some(p) = input.pointer
            && let Some(h) = self.hits.iter().rev().find(|h| h.scroll && h.layer >= min_layer && h.rect.contains(p))
            && let Some(s) = self.scrolls.get_mut(&h.id)
        {
            s.target = (s.target - input.scroll.y).clamp(0.0, s.max());
        }
        // Pouce de la barre de défilement glissé.
        let delta = match (input.pointer, self.prev_pointer) {
            (Some(a), Some(b)) => a - b,
            _ => Vec2::default(),
        };
        for (id, s) in self.scrolls.iter_mut() {
            if self.active == Some(id.with("pouce")) && s.view > 0.0 {
                s.target = (s.target + delta.y * s.content / s.view).clamp(0.0, s.max());
                s.offset = s.target;
            }
        }
        // Défilement doux.
        let k = 1.0 - (-self.dt * 22.0).exp();
        for s in self.scrolls.values_mut() {
            s.target = s.target.clamp(0.0, s.max());
            if (s.target - s.offset).abs() > 0.25 {
                s.offset += (s.target - s.offset) * k;
                self.animating = true;
            } else {
                s.offset = s.target;
            }
        }
        self.input = input;
        self.cursor = self
            .hovered
            .and_then(|id| self.hits.iter().rev().find(|h| h.id == id))
            .map(|h| h.cursor)
            .unwrap_or_default();
        if self.active.is_some_and(|a| self.scrolls.keys().any(|k| k.with("pouce") == a)) {
            self.cursor = Cursor::Grabbing;
        }
    }

    pub fn hovered(&self, id: Id) -> bool {
        self.hovered == Some(id) && (self.active.is_none() || self.active == Some(id))
    }
    pub fn hovered_raw(&self) -> Option<Id> {
        self.hovered
    }
    pub fn pressed(&self, id: Id) -> bool {
        self.active == Some(id) && self.hovered == Some(id)
    }
    /// L'appui sur `id` commence à cette image.
    pub fn press_started(&self, id: Id) -> bool {
        self.press_started && self.active == Some(id)
    }
    pub fn is_active(&self, id: Id) -> bool {
        self.active == Some(id)
    }
    pub fn clicked(&self, id: Id) -> bool {
        self.clicked == Some(id)
    }
    pub fn double_clicked(&self, id: Id) -> bool {
        self.double_clicked == Some(id)
    }
    /// Déplacement du pointeur depuis l'image précédente.
    pub fn pointer_delta(&self) -> Vec2 {
        match (self.input.pointer, self.prev_pointer) {
            (Some(a), Some(b)) => a - b,
            _ => Vec2::default(),
        }
    }
    /// Pointeur pendant le glisser de `id` (bouton gauche maintenu depuis un appui sur `id`).
    pub fn dragging(&self, id: Id) -> Option<Vec2> {
        if self.active == Some(id) { self.input.pointer } else { None }
    }
    /// Rectangle de l'élément `id` à la dernière mise en page.
    pub fn rect_of(&self, id: Id) -> Option<Rect> {
        self.hits.iter().rev().find(|h| h.id == id).map(|h| h.rect)
    }
    /// Le pointeur est-il sur l'interface (la vue 3D ne reçoit alors pas la souris) ?
    pub fn pointer_over_ui(&self) -> bool {
        self.hovered.is_some() || self.modal_layer.is_some()
    }
    /// Échap pressé et pas déjà utilisé pour fermer un menu ou quitter un champ.
    pub fn escape_pressed(&self) -> bool {
        !self.escape_used && self.input.keys.contains(&Key::Escape)
    }
    pub fn any_popup_open(&self) -> bool {
        self.open_popup.is_some()
    }
    pub fn modal_open(&self) -> bool {
        self.modal_layer.is_some()
    }

    // ---- menus et fenêtres ----

    /// Ouvre ou ferme le menu `id` (un seul menu ouvert à la fois).
    pub fn toggle_popup(&mut self, id: Id) {
        if self.open_popup == Some(id) || self.closed_popup == Some(id) {
            self.open_popup = None;
        } else {
            self.open_popup = Some(id);
        }
    }
    pub fn close_popup(&mut self) {
        self.open_popup = None;
    }
    pub fn popup_open(&self, id: Id) -> bool {
        self.open_popup == Some(id)
    }
    /// Ajoute un calque au-dessus de l'interface (menu, fenêtre), dans l'ordre d'appel.
    pub fn overlay(&mut self, node: Node, kind: Layer) {
        self.overlays.push(Overlay { node, kind });
    }

    // ---- focus et saisie ----

    pub fn focus(&mut self, id: Id) {
        if self.focus != Some(id) {
            self.focus = Some(id);
            self.focus_since = self.input.time;
        }
    }
    pub fn has_focus(&self, id: Id) -> bool {
        self.focus == Some(id)
    }
    pub fn blur(&mut self) {
        self.focus = None;
    }
    pub fn set_edit_state(&mut self, id: Id, st: EditState) {
        self.edits.insert(id, st);
    }
    pub fn edit_state(&self, id: Id) -> EditState {
        self.edits.get(&id).copied().unwrap_or_default()
    }
    /// Le curseur clignotant est-il visible ?
    pub fn caret_visible(&self) -> bool {
        ((self.input.time - self.focus_since) * 1.9) as i64 % 2 == 0
    }

    /// Applique la saisie au texte du champ `id` s'il a le focus. Renvoie vrai si le texte a changé.
    pub fn text_edit(&mut self, id: Id, text: &mut String) -> bool {
        if self.clicked(id) || self.pressed(id) {
            self.focus(id);
        }
        if self.focus != Some(id) {
            return false;
        }
        self.animating = true;
        let mut st = self.edits.get(&id).copied().unwrap_or(EditState { caret: text.chars().count(), anchor: 0 });
        let len = text.chars().count();
        st.caret = st.caret.min(len);
        st.anchor = st.anchor.min(len);
        let shift = self.input.modifiers.shift;
        let ctrl = self.input.modifiers.ctrl;
        let mut changed = false;
        let byte = |t: &str, c: usize| t.char_indices().nth(c).map(|(b, _)| b).unwrap_or(t.len());
        let delete_sel = |t: &mut String, st: &mut EditState| -> bool {
            if st.caret == st.anchor {
                return false;
            }
            let (a, b) = (st.caret.min(st.anchor), st.caret.max(st.anchor));
            let (ba, bb) = (byte(t, a), byte(t, b));
            t.replace_range(ba..bb, "");
            st.caret = a;
            st.anchor = a;
            true
        };
        for k in self.input.keys.clone() {
            let len = text.chars().count();
            match k {
                Key::Left => {
                    if st.caret != st.anchor && !shift {
                        st.caret = st.caret.min(st.anchor);
                    } else {
                        st.caret = st.caret.saturating_sub(1);
                    }
                }
                Key::Right => {
                    if st.caret != st.anchor && !shift {
                        st.caret = st.caret.max(st.anchor);
                    } else {
                        st.caret = (st.caret + 1).min(len);
                    }
                }
                Key::Home => st.caret = 0,
                Key::End => st.caret = len,
                Key::Backspace => {
                    if !delete_sel(text, &mut st) && st.caret > 0 {
                        let (a, b) = (byte(text, st.caret - 1), byte(text, st.caret));
                        text.replace_range(a..b, "");
                        st.caret -= 1;
                    }
                    changed = true;
                }
                Key::Delete => {
                    if !delete_sel(text, &mut st) && st.caret < len {
                        let (a, b) = (byte(text, st.caret), byte(text, st.caret + 1));
                        text.replace_range(a..b, "");
                    }
                    changed = true;
                }
                Key::Char('a') if ctrl => {
                    st.anchor = 0;
                    st.caret = len;
                    continue;
                }
                Key::Enter | Key::Escape => {
                    self.focus = None;
                }
                _ => continue,
            }
            if !shift || matches!(k, Key::Backspace | Key::Delete) {
                st.anchor = st.caret;
            }
        }
        if !self.input.text.is_empty() && !ctrl {
            let typed: String = self.input.text.chars().filter(|c| !c.is_control()).collect();
            if !typed.is_empty() {
                delete_sel(text, &mut st);
                let b = byte(text, st.caret);
                text.insert_str(b, &typed);
                st.caret += typed.chars().count();
                st.anchor = st.caret;
                changed = true;
            }
        }
        if changed {
            self.focus_since = self.input.time;
        }
        self.edits.insert(id, st);
        changed
    }

    // ---- animations ----

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

    /// Une nouvelle image est-elle nécessaire sans nouvelle entrée (animation, info-bulle, curseur) ?
    pub fn wants_redraw(&self) -> bool {
        self.animating
            || self.focus.is_some()
            || (self.hovered.is_some()
                && self.input.time - self.hover_since < TOOLTIP_DELAY + 0.05
                && self.hit_tooltip(self.hovered.unwrap()).is_some())
    }

    fn hit_tooltip(&self, id: Id) -> Option<(Rect, Tooltip)> {
        self.hits.iter().rev().find(|h| h.id == id).and_then(|h| h.tooltip.clone().map(|t| (h.rect, t)))
    }

    /// Couleur d'un fond selon l'état (repos, survol, pression), avec transition douce.
    pub fn state_color(&mut self, id: Id, rest: Color, hover: Color, press: Color) -> Color {
        let h = self.anim(id.with("survol"), if self.hovered(id) { 1.0 } else { 0.0 }, 90.0);
        let p = if self.pressed(id) { 1.0 } else { 0.0 };
        rest.mix(hover, h).mix(press, p)
    }

    // ---- fin d'image ----

    /// Place l'arbre et les calques dans la fenêtre, dessine, puis les info-bulles par-dessus.
    pub fn finish(&mut self, root: &mut Node) {
        let scale = self.input.scale.max(0.5);
        self.dl.clear(scale);
        let window = Rect { min: Vec2::default(), max: self.input.size };
        let mut overlays = std::mem::take(&mut self.overlays);
        // Les menus passent au-dessus des fenêtres qui les contiennent.
        overlays.sort_by_key(|o| o.kind == Layer::Popup);
        self.modal_layer = overlays.iter().rposition(|o| o.kind == Layer::Modal).map(|i| (i + 1) as u8);
        self.popup_layers =
            overlays.iter().enumerate().filter(|(_, o)| o.kind == Layer::Popup).map(|(i, _)| (i + 1) as u8).collect();
        {
            let mut res = Resources {
                fonts: &mut self.fonts,
                icons: &mut self.icons,
                atlas: &mut self.atlas,
                scale,
                scrolls: &mut self.scrolls,
            };
            root.layout(window, &mut res);
            for o in &mut overlays {
                o.node.layout(window, &mut res);
            }
        }
        self.hits.clear();
        root.collect_hits(window, 0, &mut self.hits);
        for (i, o) in overlays.iter().enumerate() {
            o.node.collect_hits(window, (i + 1) as u8, &mut self.hits);
        }
        let (hovered, active, time) = (self.hovered, self.active, self.input.time);
        {
            let mut p = Painter {
                dl: &mut self.dl,
                res: Resources {
                    fonts: &mut self.fonts,
                    icons: &mut self.icons,
                    atlas: &mut self.atlas,
                    scale,
                    scrolls: &mut self.scrolls,
                },
                theme: &self.theme,
                hovered,
                active,
                time,
            };
            root.paint(&mut p);
            for o in &overlays {
                if o.kind == Layer::Modal {
                    p.dl.rect(window, 0.0, p.theme.color.scrim, None);
                }
                o.node.paint(&mut p);
            }
        }
        let tip_id = self.force_tooltip.or_else(|| {
            let h = self.hovered?;
            (self.active.is_none() && self.input.time - self.hover_since >= TOOLTIP_DELAY).then_some(h)
        });
        if let Some(id) = tip_id
            && let Some((r, t)) = self.hit_tooltip(id)
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
}
