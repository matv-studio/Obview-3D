//! Arbre de nœuds reconstruit à chaque changement d'état, mesuré puis placé (boîtes flexibles), puis dessiné.

use crate::{
    Color, Corners, DrawList, Edges, Fill, Id, Rect, TextStyle, Vec2,
    ui::{Cursor, Hit, Painter, Resources},
    vec2,
};

/// Largeur réservée à droite d'une zone défilante quand son contenu déborde.
pub const SCROLLBAR_GUTTER: f32 = 10.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Size {
    /// Taille du contenu.
    #[default]
    Auto,
    Px(f32),
    /// Part de l'espace restant le long de l'axe principal du parent (ou tout l'espace sur l'axe croisé).
    Fill(f32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Dir {
    #[default]
    Row,
    Column,
    /// Les enfants se superposent, chacun occupant toute la place.
    Stack,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Align {
    Start,
    #[default]
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}

/// Placement hors flux, relatif au parent (comme `position: absolute`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Anchor {
    pub left: Option<f32>,
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    /// Centrage horizontal (ignore `left`/`right`).
    pub center_x: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Shadow {
    pub blur: f32,
    pub offset: Vec2,
    pub color: Color,
}

#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub dir: Dir,
    pub width: Size,
    pub height: Size,
    pub min_width: f32,
    pub padding: Edges,
    pub gap: f32,
    pub align: Align,
    pub justify: Justify,
    pub anchor: Option<Anchor>,
    pub bg: Option<Fill>,
    pub border: Option<(f32, Color)>,
    /// Bordure d'un seul côté, dessinée comme un trait (séparateurs de panneaux).
    pub border_bottom: Option<Color>,
    pub border_top: Option<Color>,
    pub border_right: Option<Color>,
    pub border_left: Option<Color>,
    pub radius: Corners,
    pub shadows: [Option<Shadow>; 2],
    pub clip: bool,
    pub opacity: f32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            dir: Dir::Row,
            width: Size::Auto,
            height: Size::Auto,
            min_width: 0.0,
            padding: Edges::default(),
            gap: 0.0,
            align: Align::Center,
            justify: Justify::Start,
            anchor: None,
            bg: None,
            border: None,
            border_bottom: None,
            border_top: None,
            border_right: None,
            border_left: None,
            radius: Corners::default(),
            shadows: [None, None],
            clip: false,
            opacity: 1.0,
        }
    }
}

pub type CustomPaint = Box<dyn Fn(&mut Painter<'_>, Rect)>;

pub enum Content {
    None,
    Text {
        text: String,
        style: TextStyle,
        color: Color,
        truncate: bool,
    },
    Icon {
        name: &'static str,
        size: f32,
        color: Color,
    },
    /// Dessin libre dans le rectangle du nœud.
    Custom(CustomPaint),
}

pub struct Node {
    pub id: Option<Id>,
    pub tooltip: Option<Tooltip>,
    pub style: Style,
    pub content: Content,
    pub children: Vec<Node>,
    pub rect: Rect,
    /// Surface sensible plutôt qu'un élément purement décoratif (reçoit le survol même sans identifiant explicite).
    pub interactive: bool,
    /// Forme du pointeur au survol.
    pub cursor: Cursor,
    /// Zone défilante verticale : identifiant de son état.
    pub scroll: Option<Id>,
    /// Pouce de la barre de défilement (calculé à la mise en page).
    thumb: Option<Rect>,
}

#[derive(Clone, Debug)]
pub struct Tooltip {
    pub text: String,
    pub shortcut: Option<String>,
}

impl Node {
    pub fn new() -> Self {
        Node {
            id: None,
            tooltip: None,
            style: Style::default(),
            content: Content::None,
            children: Vec::new(),
            rect: Rect::ZERO,
            interactive: false,
            cursor: Cursor::Default,
            scroll: None,
            thumb: None,
        }
    }
    /// Colonne défilante verticalement, découpée, avec une barre de défilement maison.
    pub fn scroll(id: Id) -> Self {
        let mut n = Self::column().clip();
        n.scroll = Some(id);
        n
    }
    pub fn row() -> Self {
        Self::new().dir(Dir::Row)
    }
    pub fn column() -> Self {
        Self::new().dir(Dir::Column).align(Align::Stretch)
    }
    pub fn stack() -> Self {
        Self::new().dir(Dir::Stack)
    }
    pub fn text(text: impl Into<String>, style: TextStyle, color: Color) -> Self {
        let mut n = Self::new();
        n.content = Content::Text { text: text.into(), style, color, truncate: false };
        n
    }
    pub fn icon(name: &'static str, size: f32, color: Color) -> Self {
        let mut n = Self::new().size(Size::Px(size), Size::Px(size));
        n.content = Content::Icon { name, size, color };
        n
    }
    pub fn custom(paint: impl Fn(&mut Painter<'_>, Rect) + 'static) -> Self {
        let mut n = Self::new();
        n.content = Content::Custom(Box::new(paint));
        n
    }
    pub fn spacer() -> Self {
        Self::new().width(Size::Fill(1.0))
    }

    pub fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self.interactive = true;
        self
    }
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(Tooltip { text: text.into(), shortcut: None });
        self
    }
    pub fn tooltip_shortcut(mut self, text: impl Into<String>, shortcut: impl Into<String>) -> Self {
        self.tooltip = Some(Tooltip { text: text.into(), shortcut: Some(shortcut.into()) });
        self
    }
    pub fn cursor(mut self, c: Cursor) -> Self {
        self.cursor = c;
        self
    }
    pub fn dir(mut self, d: Dir) -> Self {
        self.style.dir = d;
        self
    }
    pub fn width(mut self, w: Size) -> Self {
        self.style.width = w;
        self
    }
    pub fn height(mut self, h: Size) -> Self {
        self.style.height = h;
        self
    }
    pub fn size(mut self, w: Size, h: Size) -> Self {
        self.style.width = w;
        self.style.height = h;
        self
    }
    pub fn min_width(mut self, w: f32) -> Self {
        self.style.min_width = w;
        self
    }
    pub fn fill(self) -> Self {
        self.size(Size::Fill(1.0), Size::Fill(1.0))
    }
    pub fn padding(mut self, p: Edges) -> Self {
        self.style.padding = p;
        self
    }
    pub fn gap(mut self, g: f32) -> Self {
        self.style.gap = g;
        self
    }
    pub fn align(mut self, a: Align) -> Self {
        self.style.align = a;
        self
    }
    pub fn justify(mut self, j: Justify) -> Self {
        self.style.justify = j;
        self
    }
    pub fn anchor(mut self, a: Anchor) -> Self {
        self.style.anchor = Some(a);
        self
    }
    pub fn bg(mut self, f: impl Into<Fill>) -> Self {
        self.style.bg = Some(f.into());
        self
    }
    pub fn border(mut self, w: f32, c: Color) -> Self {
        self.style.border = Some((w, c));
        self
    }
    pub fn border_bottom(mut self, c: Color) -> Self {
        self.style.border_bottom = Some(c);
        self
    }
    pub fn border_top(mut self, c: Color) -> Self {
        self.style.border_top = Some(c);
        self
    }
    pub fn border_right(mut self, c: Color) -> Self {
        self.style.border_right = Some(c);
        self
    }
    pub fn border_left(mut self, c: Color) -> Self {
        self.style.border_left = Some(c);
        self
    }
    pub fn radius(mut self, r: impl Into<Corners>) -> Self {
        self.style.radius = r.into();
        self
    }
    pub fn shadow(mut self, blur: f32, offset: Vec2, color: Color) -> Self {
        let s = Some(Shadow { blur, offset, color });
        if self.style.shadows[0].is_none() {
            self.style.shadows[0] = s;
        } else {
            self.style.shadows[1] = s;
        }
        self
    }
    pub fn clip(mut self) -> Self {
        self.style.clip = true;
        self
    }
    pub fn opacity(mut self, o: f32) -> Self {
        self.style.opacity = o;
        self
    }
    pub fn truncate(mut self) -> Self {
        if let Content::Text { truncate, .. } = &mut self.content {
            *truncate = true;
        }
        self
    }
    pub fn child(mut self, c: Node) -> Self {
        self.children.push(c);
        self
    }
    pub fn children(mut self, cs: impl IntoIterator<Item = Node>) -> Self {
        self.children.extend(cs);
        self
    }
    pub fn push(&mut self, c: Node) {
        self.children.push(c);
    }

    // ---- mesure ----

    fn content_size(&self, res: &mut Resources<'_>) -> Vec2 {
        match &self.content {
            Content::Text { text, style, .. } => {
                let l = res.fonts.layout(text, *style, res.scale);
                vec2(l.width.ceil(), l.height)
            }
            Content::Icon { size, .. } => vec2(*size, *size),
            _ => Vec2::default(),
        }
    }

    /// Taille souhaitée (contenu + marges), hors tailles `Fill`.
    fn measure(&self, res: &mut Resources<'_>) -> Vec2 {
        let p = self.style.padding;
        let mut inner = self.content_size(res);
        let flow: Vec<&Node> = self.children.iter().filter(|c| c.style.anchor.is_none()).collect();
        if !flow.is_empty() {
            let sizes: Vec<Vec2> = flow.iter().map(|c| c.measure_outer(res)).collect();
            let gaps = self.style.gap * (sizes.len().saturating_sub(1)) as f32;
            inner = match self.style.dir {
                Dir::Row => {
                    vec2(sizes.iter().map(|s| s.x).sum::<f32>() + gaps, sizes.iter().fold(0.0f32, |m, s| m.max(s.y)))
                }
                Dir::Column => {
                    vec2(sizes.iter().fold(0.0f32, |m, s| m.max(s.x)), sizes.iter().map(|s| s.y).sum::<f32>() + gaps)
                }
                Dir::Stack => sizes.iter().fold(Vec2::default(), |m, s| vec2(m.x.max(s.x), m.y.max(s.y))),
            };
        }
        vec2(inner.x + p.horizontal(), inner.y + p.vertical())
    }

    fn measure_outer(&self, res: &mut Resources<'_>) -> Vec2 {
        let m = self.measure(res);
        let w = match self.style.width {
            Size::Px(v) => v,
            Size::Auto => m.x.max(self.style.min_width),
            Size::Fill(_) => self.style.min_width,
        };
        let h = match self.style.height {
            Size::Px(v) => v,
            Size::Auto => m.y,
            Size::Fill(_) => 0.0,
        };
        vec2(w, h)
    }

    // ---- placement ----

    pub fn layout(&mut self, rect: Rect, res: &mut Resources<'_>) {
        self.rect = rect;
        self.thumb = None;
        let mut inner = rect.shrink(self.style.padding);
        if let Some(sid) = self.scroll {
            let content = self.measure(res).y;
            let view = rect.height();
            let st = res.scrolls.entry(sid).or_default();
            st.content = content;
            st.view = view;
            let max = st.max();
            st.target = st.target.clamp(0.0, max);
            st.offset = st.offset.clamp(0.0, max);
            let off = st.offset.round();
            // Gouttière toujours réservée : la mise en page ne saute pas quand le contenu commence à déborder.
            inner.max.x -= SCROLLBAR_GUTTER;
            if max > 0.0 {
                let track = Rect::from_xywh(rect.max.x - 8.0, rect.min.y + 3.0, 6.0, view - 6.0);
                let h = (track.height() * view / content).max(28.0).min(track.height());
                let y = track.min.y + (track.height() - h) * (st.offset / max);
                self.thumb = Some(Rect::from_xywh(track.min.x, y, track.width(), h));
            }
            inner = Rect::from_xywh(
                inner.min.x,
                inner.min.y - off,
                inner.width(),
                (content - self.style.padding.vertical()).max(inner.height()),
            );
        }
        let dir = self.style.dir;
        let measured: Vec<Vec2> = self.children.iter().map(|c| c.measure_outer(res)).collect();

        // Enfants hors flux.
        for (c, m) in self.children.iter_mut().zip(&measured) {
            if let Some(a) = c.style.anchor {
                let w = match c.style.width {
                    Size::Fill(_) => inner.width() - a.left.unwrap_or(0.0) - a.right.unwrap_or(0.0),
                    _ => m.x,
                };
                let h = match c.style.height {
                    Size::Fill(_) => inner.height() - a.top.unwrap_or(0.0) - a.bottom.unwrap_or(0.0),
                    _ => m.y,
                };
                let x = if a.center_x {
                    inner.min.x + (inner.width() - w) * 0.5
                } else if let Some(l) = a.left {
                    inner.min.x + l
                } else if let Some(r) = a.right {
                    inner.max.x - r - w
                } else {
                    inner.min.x
                };
                let y = if let Some(t) = a.top {
                    inner.min.y + t
                } else if let Some(b) = a.bottom {
                    inner.max.y - b - h
                } else {
                    inner.min.y
                };
                c.layout(Rect::from_xywh(x, y, w, h), res);
            }
        }

        let flow: Vec<usize> = (0..self.children.len()).filter(|&i| self.children[i].style.anchor.is_none()).collect();
        if flow.is_empty() {
            return;
        }
        if dir == Dir::Stack {
            for &i in &flow {
                let c = &mut self.children[i];
                // Taille du contenu centrée, ou toute la place pour `Fill`.
                let w = if matches!(c.style.width, Size::Fill(_)) {
                    inner.width()
                } else {
                    measured[i].x.min(inner.width())
                };
                let h = if matches!(c.style.height, Size::Fill(_)) {
                    inner.height()
                } else {
                    measured[i].y.min(inner.height())
                };
                let x = inner.min.x + (inner.width() - w) * 0.5;
                let y = inner.min.y + (inner.height() - h) * 0.5;
                c.layout(Rect::from_xywh(x, y, w, h), res);
            }
            return;
        }
        let row = dir == Dir::Row;
        let main_total = if row { inner.width() } else { inner.height() };
        let cross_total = if row { inner.height() } else { inner.width() };
        let main_of = |v: Vec2| if row { v.x } else { v.y };
        let cross_of = |v: Vec2| if row { v.y } else { v.x };
        let size_main = |n: &Node| if row { n.style.width } else { n.style.height };
        let size_cross = |n: &Node| if row { n.style.height } else { n.style.width };

        let gaps = self.style.gap * (flow.len() - 1) as f32;
        let mut fixed = 0.0;
        let mut weights = 0.0;
        for &i in &flow {
            match size_main(&self.children[i]) {
                Size::Fill(w) => {
                    weights += w;
                    fixed += main_of(measured[i]);
                }
                _ => fixed += main_of(measured[i]),
            }
        }
        let free = (main_total - fixed - gaps).max(0.0);
        let mains: Vec<f32> = flow
            .iter()
            .map(|&i| match size_main(&self.children[i]) {
                Size::Fill(w) if weights > 0.0 => main_of(measured[i]) + free * w / weights,
                _ => main_of(measured[i]),
            })
            .collect();
        let used: f32 = mains.iter().sum::<f32>() + gaps;
        let leftover = (main_total - used).max(0.0);
        let (mut cursor, extra_gap) = match self.style.justify {
            Justify::Start => (0.0, 0.0),
            Justify::Center => (leftover * 0.5, 0.0),
            Justify::End => (leftover, 0.0),
            Justify::SpaceBetween if flow.len() > 1 => (0.0, leftover / (flow.len() - 1) as f32),
            Justify::SpaceBetween => (0.0, 0.0),
        };
        for (k, &i) in flow.iter().enumerate() {
            let c = &mut self.children[i];
            let main = mains[k];
            let cross = match size_cross(c) {
                Size::Fill(_) => cross_total,
                Size::Px(v) => v,
                Size::Auto if self.style.align == Align::Stretch => cross_total,
                Size::Auto => cross_of(measured[i]).min(cross_total),
            };
            let off = match self.style.align {
                Align::Start | Align::Stretch => 0.0,
                Align::Center => ((cross_total - cross) * 0.5).round(),
                Align::End => cross_total - cross,
            };
            let r = if row {
                Rect::from_xywh(inner.min.x + cursor, inner.min.y + off, main, cross)
            } else {
                Rect::from_xywh(inner.min.x + off, inner.min.y + cursor, cross, main)
            };
            c.layout(r, res);
            cursor += main + self.style.gap + extra_gap;
        }
    }

    // ---- dessin et zones sensibles ----

    pub fn paint(&self, p: &mut Painter<'_>) {
        let r = self.rect;
        if r.is_empty() && self.children.is_empty() {
            return;
        }
        let s = &self.style;
        let fade = |c: Color| c.mul_alpha(s.opacity);
        for sh in s.shadows.iter().flatten() {
            p.dl.shadow(r, s.radius.tl, sh.blur, sh.offset, fade(sh.color));
        }
        if s.bg.is_some() || s.border.is_some() {
            let fill = match s.bg {
                Some(Fill::Solid(c)) => Fill::Solid(fade(c)),
                Some(Fill::Vertical(a, b)) => Fill::Vertical(fade(a), fade(b)),
                None => Fill::Solid(Color::TRANSPARENT),
            };
            p.dl.rect(r, s.radius, fill, s.border.map(|(w, c)| (w, fade(c))));
        }
        let px = 1.0 / p.dl.scale;
        if let Some(c) = s.border_bottom {
            p.dl.rect(Rect::from_xywh(r.min.x, r.max.y - px, r.width(), px), 0.0, fade(c), None);
        }
        if let Some(c) = s.border_top {
            p.dl.rect(Rect::from_xywh(r.min.x, r.min.y, r.width(), px), 0.0, fade(c), None);
        }
        if let Some(c) = s.border_right {
            p.dl.rect(Rect::from_xywh(r.max.x - px, r.min.y, px, r.height()), 0.0, fade(c), None);
        }
        if let Some(c) = s.border_left {
            p.dl.rect(Rect::from_xywh(r.min.x, r.min.y, px, r.height()), 0.0, fade(c), None);
        }
        if s.clip {
            p.dl.push_clip(r);
        }
        let inner = r.shrink(s.padding);
        match &self.content {
            Content::None => {}
            Content::Text { text, style, color, truncate } => {
                let layout = if *truncate {
                    p.res.fonts.layout_truncated(text, *style, p.dl.scale, inner.width())
                } else {
                    p.res.fonts.layout(text, *style, p.dl.scale)
                };
                let y = inner.min.y + ((inner.height() - layout.height) * 0.5).round();
                p.res.fonts.draw(p.dl, p.res.atlas, &layout, vec2(inner.min.x, y), fade(*color));
            }
            Content::Icon { name, size, color } => {
                p.res.icons.draw(p.dl, p.res.atlas, name, inner, *size, fade(*color));
            }
            Content::Custom(f) => f(p, inner),
        }
        for c in &self.children {
            c.paint(p);
        }
        if s.clip {
            p.dl.pop_clip();
        }
        if let (Some(t), Some(sid)) = (self.thumb, self.scroll) {
            // Barre de défilement de la charte : trait fin sombre sur toute la hauteur, partie visible en pilule.
            let tid = sid.with("pouce");
            let hot = p.hovered == Some(tid) || p.active == Some(tid);
            let cx = t.center().x;
            let track = Rect::from_xywh(cx - 1.0, r.min.y + 3.0, 2.0, r.height() - 6.0);
            p.dl.rect(track, 1.0, p.theme.color.track, None);
            let color = if hot { p.theme.color.thumb_hover } else { p.theme.color.thumb };
            let pill = Rect::from_xywh(cx - 2.0, t.min.y, 4.0, t.height());
            p.dl.rect(pill, 2.0, color, None);
        }
    }

    /// Zones sensibles dans l'ordre de dessin (la dernière est au-dessus), avec la découpe appliquée.
    pub fn collect_hits(&self, clip: Rect, layer: u8, out: &mut Vec<Hit>) {
        let visible = self.rect.intersect(&clip);
        if self.style.clip && visible.is_empty() {
            return;
        }
        if let Some(id) = self.id
            && !visible.is_empty()
        {
            out.push(Hit {
                id,
                rect: visible,
                tooltip: self.tooltip.clone(),
                cursor: self.cursor,
                layer,
                scroll: false,
            });
        }
        if let Some(sid) = self.scroll
            && !visible.is_empty()
        {
            out.push(Hit { id: sid, rect: visible, tooltip: None, cursor: Cursor::Default, layer, scroll: true });
        }
        let child_clip = if self.style.clip { visible } else { clip };
        for c in &self.children {
            c.collect_hits(child_clip, layer, out);
        }
        if let (Some(t), Some(sid)) = (self.thumb, self.scroll) {
            let r = t.expand(2.0).intersect(&clip);
            out.push(Hit {
                id: sid.with("pouce"),
                rect: r,
                tooltip: None,
                cursor: Cursor::Default,
                layer,
                scroll: false,
            });
        }
    }
}

impl Default for Node {
    fn default() -> Self {
        Self::new()
    }
}

/// Ajoute un rectangle d'un pixel physique (séparateur net à toute échelle).
pub fn hairline(dl: &mut DrawList, rect: Rect, color: Color) {
    dl.rect(rect, 0.0, color, None);
}
