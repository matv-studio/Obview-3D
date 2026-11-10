//! Outils de la vue 3D (D-26) : mesure, coupe, vue éclatée, capture. État, panneau de l'outil actif (en haut à
//! gauche de la vue) et dessins par-dessus la scène (cotes, plan de coupe).

use obv_render::Camera;
use obv_ui::{
    Anchor, Color, Cursor, Edges, Id, Layer, Node, Rect, Size, TextStyle, Ui, Vec2, Weight, controls, i18n, tr, vec2,
    widgets::{self, ButtonKind},
};

use crate::shell::{AppState, Tool};

/// Point de mesure : pièce et point dans le repère de la pièce (avant éclatement).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasurePoint {
    pub part: usize,
    pub local: [f64; 3],
}

/// Accroche du point survolé.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Snap {
    Vertex,
    Edge,
    Surface,
}

#[derive(Clone, Debug, Default)]
pub struct MeasureState {
    /// Mesures terminées : deux points.
    pub done: Vec<[MeasurePoint; 2]>,
    /// Premier point de la mesure en cours.
    pub pending: Option<MeasurePoint>,
    /// Point sous le pointeur (calculé par la fenêtre, avec accroche).
    pub hover: Option<(MeasurePoint, Snap)>,
}

#[derive(Clone, Copy, Debug)]
pub struct SectionState {
    /// Axe du plan : 0 X, 1 Y, 2 Z.
    pub axis: usize,
    /// Côté gardé inversé.
    pub flipped: bool,
    /// Position du plan dans la boîte englobante, 0 à 1.
    pub position: f32,
}

impl Default for SectionState {
    fn default() -> Self {
        SectionState { axis: 0, flipped: false, position: 0.5 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CaptureSize {
    #[default]
    View,
    Double,
    Uhd,
}

impl CaptureSize {
    pub const ALL: [CaptureSize; 3] = [CaptureSize::View, CaptureSize::Double, CaptureSize::Uhd];
    pub fn label(self) -> &'static str {
        match self {
            CaptureSize::View => tr("Vue"),
            CaptureSize::Double => "2×",
            CaptureSize::Uhd => "4K",
        }
    }
    /// Taille en pixels pour une vue de `w` × `h` pixels de l'écran.
    pub fn pixels(self, w: u32, h: u32) -> (u32, u32) {
        match self {
            CaptureSize::View => (w, h),
            CaptureSize::Double => (w * 2, h * 2),
            CaptureSize::Uhd => (3840, 2160),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CaptureState {
    pub size: CaptureSize,
    pub transparent: bool,
}

/// Plan de coupe dans le repère du modèle (normale unitaire, distance), ou rien hors de l'outil Coupe.
pub fn section_plane(s: &AppState) -> Option<[f32; 4]> {
    if s.tool != Tool::Section {
        return None;
    }
    let (lo, hi) = s.scene_bounds?;
    let a = s.section.axis.min(2);
    let x = lo[a] + (hi[a] - lo[a]) * s.section.position;
    let mut n = [0.0f32; 3];
    n[a] = if s.section.flipped { -1.0 } else { 1.0 };
    Some([n[0], n[1], n[2], n[a] * x])
}

/// Décalage de chaque pièce pour la vue éclatée : du centre de l'assemblage vers le centre de la pièce.
pub fn explode_offsets(s: &AppState) -> Vec<[f32; 3]> {
    if s.explode <= 0.0 {
        return Vec::new();
    }
    let Some((lo, hi)) = s.scene_bounds else { return Vec::new() };
    let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
    let k = s.explode * 1.5;
    s.parts.iter().map(|p| std::array::from_fn(|i| (p.center[i] - c[i]) * k)).collect()
}

/// Point du monde d'un point de mesure (avec l'éclatement courant).
pub fn world(p: &MeasurePoint, offsets: &[[f32; 3]]) -> [f64; 3] {
    let o = offsets.get(p.part).copied().unwrap_or([0.0; 3]);
    [p.local[0] + o[0] as f64, p.local[1] + o[1] as f64, p.local[2] + o[2] as f64]
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Projection à l'écran (pixels logiques) d'un point du monde dans la vue `view`.
pub fn to_screen(cam: &Camera, view: Rect, p: [f64; 3]) -> Option<Vec2> {
    let aspect = (view.width() / view.height().max(1.0)) as f64;
    let (n, front) = cam.project(aspect, p);
    front.then(|| {
        vec2(
            view.min.x + ((n[0] + 1.0) * 0.5) as f32 * view.width(),
            view.min.y + ((1.0 - n[1]) * 0.5) as f32 * view.height(),
        )
    })
}

fn id(name: &str) -> Id {
    Id::new("outils").with(name)
}

/// Panneau d'outil flottant ; `appear` (0 → 1) : fondu et glissement de 6 px vers le bas à l'ouverture.
fn panel(ui: &Ui, appear: f32, rows: Vec<Node>) -> Node {
    let c = &ui.theme.color;
    Node::column()
        .anchor(Anchor { left: Some(12.0), top: Some(12.0 - (1.0 - appear) * 6.0), ..Default::default() })
        .opacity(appear)
        .width(Size::Px(264.0))
        .padding(Edges::xy(12.0, 10.0))
        .gap(8.0)
        .radius(ui.theme.size.radius_panel)
        .bg(c.floating)
        .border(1.0, c.border_strong)
        .shadow(24.0, vec2(0.0, 10.0), c.shadow)
        .children(rows)
}

fn title(ui: &Ui, icon: &'static str, text: &str) -> Node {
    let c = &ui.theme.color;
    Node::row().gap(8.0).height(Size::Px(20.0)).child(Node::icon(icon, 15.0, c.text_secondary)).child(Node::text(
        text,
        TextStyle::new(13.0, Weight::SemiBold),
        c.text,
    ))
}

fn value_row(ui: &Ui, label: &str, value: String) -> Node {
    let c = &ui.theme.color;
    Node::row()
        .width(Size::Fill(1.0))
        .height(Size::Px(22.0))
        .child(Node::text(label, TextStyle::new(12.5, Weight::Regular), c.text_secondary).width(Size::Fill(1.0)))
        .child(Node::text(value, TextStyle::new(12.5, Weight::Medium), c.text))
}

/// Choix segmenté : le nœud et l'indice cliqué à cette image.
pub(crate) fn segmented(ui: &mut Ui, base: Id, items: &[&str], selected: usize) -> (Node, Option<usize>) {
    let mut chosen = None;
    for i in 0..items.len() {
        if ui.clicked(base.with(i)) {
            chosen = Some(i);
        }
    }
    (widgets::tabs(ui, base, items, chosen.unwrap_or(selected)), chosen)
}

/// Panneau de l'outil actif, en haut à gauche de la vue.
pub fn tool_panel(ui: &mut Ui, s: &mut AppState) -> Option<Node> {
    let units = s.settings.units;
    let unit_m = s.unit_m;
    let appear = ui.appear(id("panneau").with(s.tool as u8), 160.0);
    match s.tool {
        Tool::Measure => {
            if ui.clicked(id("effacer")) {
                s.measure = MeasureState::default();
            }
            let mut rows = vec![title(ui, "measure", tr("Mesurer"))];
            let last = s.measure.done.last().copied();
            match last {
                Some([a, b]) => {
                    let d: [f64; 3] = std::array::from_fn(|i| (b.local[i] - a.local[i]).abs() * unit_m);
                    rows.push(value_row(ui, tr("Distance"), units.format(distance(a.local, b.local) * unit_m)));
                    for (k, axis) in ["X", "Y", "Z"].iter().enumerate() {
                        rows.push(value_row(ui, &format!("{} {axis}", tr("Écart")), units.format(d[k])));
                    }
                }
                None => rows.push(value_row(ui, tr("Distance"), "—".into())),
            }
            let n = s.measure.done.len();
            rows.push(
                Node::row()
                    .width(Size::Fill(1.0))
                    .child(
                        Node::text(
                            i18n::pick(
                                format!("{n} measurement{}", if n == 1 { "" } else { "s" }),
                                format!("{n} mesure{}", if n > 1 { "s" } else { "" }),
                            ),
                            TextStyle::new(12.0, Weight::Regular),
                            ui.theme.color.text_tertiary,
                        )
                        .width(Size::Fill(1.0)),
                    )
                    .child(
                        widgets::button(
                            ui,
                            id("effacer"),
                            tr("Effacer"),
                            Some("trash"),
                            ButtonKind::Secondary,
                            tr("Effacer toutes les mesures (Suppr)"),
                        )
                        .opacity(if n > 0 || s.measure.pending.is_some() { 1.0 } else { 0.45 }),
                    ),
            );
            Some(panel(ui, appear, rows))
        }
        Tool::Section => {
            let (axes, picked) = segmented(ui, id("axe"), &["X", "Y", "Z"], s.section.axis);
            if let Some(a) = picked {
                s.section.axis = a;
            }
            if ui.clicked(id("inverser")) {
                s.section.flipped = !s.section.flipped;
            }
            let (slider, moved) =
                controls::slider_with_default(ui, id("position"), s.section.position, 0.0, 1.0, 0.0, 240.0, Some(0.5));
            if let Some(v) = moved {
                s.section.position = v;
            }
            let pos = s.scene_bounds.map(|(lo, hi)| {
                let a = s.section.axis.min(2);
                (lo[a] + (hi[a] - lo[a]) * s.section.position) as f64 * unit_m
            });
            let axis_row = Node::row().width(Size::Fill(1.0)).gap(6.0).child(axes.width(Size::Fill(1.0))).child(
                widgets::icon_button(ui, id("inverser"), "flip", "Inverser le côté gardé", None, s.section.flipped),
            );
            Some(panel(
                ui,
                appear,
                vec![
                    title(ui, "section", tr("Coupe")),
                    axis_row,
                    slider.width(Size::Fill(1.0)),
                    value_row(
                        ui,
                        tr(["Position X", "Position Y", "Position Z"][s.section.axis.min(2)]),
                        pos.map(|m| units.format(m)).unwrap_or_default(),
                    ),
                ],
            ))
        }
        Tool::Explode => {
            let (slider, moved) =
                controls::slider_with_default(ui, id("écartement"), s.explode, 0.0, 1.0, 0.0, 240.0, Some(0.0));
            if let Some(v) = moved {
                s.explode = v;
            }
            Some(panel(
                ui,
                appear,
                vec![
                    title(ui, "explode", tr("Vue éclatée")),
                    slider.width(Size::Fill(1.0)),
                    value_row(ui, tr("Écartement"), i18n::with_unit(&format!("{:.0}", s.explode * 100.0), "%")),
                ],
            ))
        }
        Tool::Select => None,
    }
}

/// Fenêtre de capture, au-dessus du bouton Capture.
pub fn capture_popup(ui: &mut Ui, s: &mut AppState, anchor: Option<Rect>) {
    let bid = crate::shell::tool_button_id(crate::settings::ToolButton::Capture);
    if ui.clicked(bid) {
        ui.toggle_popup(bid);
    }
    if !ui.popup_open(bid) {
        return;
    }
    let Some(r) = anchor.or_else(|| ui.rect_of(bid)) else { return };
    let c = ui.theme.color.clone();
    let sizes: Vec<&str> = CaptureSize::ALL.iter().map(|z| z.label()).collect();
    let cur = CaptureSize::ALL.iter().position(|z| *z == s.capture.size).unwrap_or(0);
    let (size_seg, picked) = segmented(ui, id("taille"), &sizes, cur);
    if let Some(i) = picked {
        s.capture.size = CaptureSize::ALL[i];
    }
    let (bg_seg, picked) = segmented(ui, id("fond"), &[tr("Fond"), tr("Transparent")], s.capture.transparent as usize);
    if let Some(i) = picked {
        s.capture.transparent = i == 1;
    }
    let save = id("enregistrer");
    if ui.clicked(save) {
        s.requests
            .push(crate::convert_ui::Request::Capture { size: s.capture.size, transparent: s.capture.transparent });
        ui.close_popup();
    }
    let row = |label: &str, ctl: Node| {
        Node::row()
            .width(Size::Fill(1.0))
            .height(Size::Px(32.0))
            .child(Node::text(label, TextStyle::new(12.5, Weight::Regular), c.text_secondary).width(Size::Fill(1.0)))
            .child(ctl.width(Size::Px(170.0)))
    };
    let w = 270.0;
    let h = 32.0 * 2.0 + 28.0 + 12.0 + 20.0;
    // Apparition 120 ms : fondu et glissement de 4 px depuis le bouton (en dessous).
    let appear = ui.appear(id("menu"), 120.0);
    let menu = Node::column()
        .anchor(Anchor {
            left: Some((r.center().x - w * 0.5).max(8.0)),
            top: Some(r.min.y - 10.0 - h + (1.0 - appear) * 4.0),
            ..Default::default()
        })
        .opacity(appear)
        .width(Size::Px(w))
        .padding(Edges::xy(12.0, 10.0))
        .gap(4.0)
        .radius(ui.theme.size.radius_panel)
        .bg(c.raised)
        .border(1.0, c.border_strong)
        .shadow(40.0, vec2(0.0, 16.0), c.shadow)
        .child(row(tr("Taille"), size_seg))
        .child(row(tr("Arrière-plan"), bg_seg))
        .child(Node::new().height(Size::Px(4.0)))
        .child(
            widgets::button(ui, save, tr("Enregistrer l'image"), Some("capture"), ButtonKind::Primary, "PNG")
                .width(Size::Fill(1.0)),
        );
    ui.overlay(Node::stack().child(menu), Layer::Popup);
}

/// Dessins par-dessus la scène : cotes, point survolé, plan de coupe et sa poignée.
pub fn overlay(ui: &mut Ui, s: &AppState) -> Option<Node> {
    let cam = s.camera;
    let theme = ui.theme.clone();
    let c = theme.color.clone();
    let hl = if c.has_accent { c.accent } else { c.set_text };
    let units = s.settings.units;
    let unit_m = s.unit_m;
    let offsets = explode_offsets(s);
    let mut layer = Node::stack().fill();
    let mut any = false;

    if !s.measure.done.is_empty() || s.tool == Tool::Measure {
        any = true;
        let done = s.measure.done.clone();
        let pending = s.measure.pending;
        let hover = if s.tool == Tool::Measure { s.measure.hover } else { None };
        let offs = offsets.clone();
        let label_bg = c.tooltip_bg;
        let label_fg = c.tooltip_text;
        layer.push(
            Node::custom(move |p, r| {
                let dot = |p: &mut obv_ui::Painter<'_>, q: Vec2, rad: f32| {
                    let rr = Rect::from_xywh(q.x - rad, q.y - rad, 2.0 * rad, 2.0 * rad);
                    p.dl.rect(rr, rad, Color::WHITE, Some((1.5, hl)));
                };
                let label = |p: &mut obv_ui::Painter<'_>, at: Vec2, text: &str| {
                    let l = p.res.fonts.layout(text, TextStyle::new(12.0, Weight::SemiBold), p.dl.scale);
                    let (w, h) = (l.width + 14.0, 22.0);
                    let box_ = Rect::from_xywh((at.x - w * 0.5).round(), (at.y - h - 8.0).round(), w, h);
                    p.dl.shadow(box_, 6.0, 8.0, vec2(0.0, 2.0), Color::BLACK.with_alpha(0.25));
                    p.dl.rect(box_, 6.0, label_bg, None);
                    let pos = vec2(box_.min.x + 7.0, box_.min.y + (h - l.height) * 0.5);
                    p.res.fonts.draw(p.dl, p.res.atlas, &l, vec2(pos.x.round(), pos.y.round()), label_fg);
                };
                p.dl.push_clip(r);
                for [a, b] in &done {
                    let (wa, wb) = (world(a, &offs), world(b, &offs));
                    if let (Some(sa), Some(sb)) = (to_screen(&cam, r, wa), to_screen(&cam, r, wb)) {
                        p.dl.line(sa, sb, 2.0, hl);
                        dot(p, sa, 4.0);
                        dot(p, sb, 4.0);
                        let d = distance(a.local, b.local) * unit_m;
                        label(p, (sa + sb) * 0.5, &units.format(d));
                    }
                }
                if let Some(a) = pending
                    && let Some(sa) = to_screen(&cam, r, world(&a, &offs))
                {
                    if let Some((h, _)) = hover
                        && let Some(sh) = to_screen(&cam, r, world(&h, &offs))
                    {
                        p.dl.line(sa, sh, 1.5, hl.with_alpha(0.8));
                        label(p, (sa + sh) * 0.5, &units.format(distance(a.local, h.local) * unit_m));
                    }
                    dot(p, sa, 4.0);
                }
                if let Some((h, snap)) = hover
                    && let Some(q) = to_screen(&cam, r, world(&h, &offs))
                {
                    match snap {
                        Snap::Vertex => {
                            let rr = Rect::from_xywh(q.x - 5.0, q.y - 5.0, 10.0, 10.0);
                            p.dl.rect(rr, 1.5, Color::TRANSPARENT, Some((2.0, hl)));
                        }
                        Snap::Edge => {
                            let pts =
                                [q + vec2(0.0, -6.0), q + vec2(6.0, 0.0), q + vec2(0.0, 6.0), q + vec2(-6.0, 0.0)];
                            p.dl.quad(pts, Color::TRANSPARENT, Some((2.0, hl)));
                        }
                        Snap::Surface => dot(p, q, 3.5),
                    }
                }
                p.dl.pop_clip();
            })
            .fill(),
        );
    }

    if s.tool == Tool::Section
        && let (Some(plane), Some((lo, hi))) = (section_plane(s), s.scene_bounds)
    {
        any = true;
        let a = s.section.axis.min(2);
        let x = plane[3] / plane[a];
        // Rectangle du plan, élargi de 6 % autour de la boîte.
        let (u, v) = ((a + 1) % 3, (a + 2) % 3);
        let m = |i: usize| (hi[i] - lo[i]) * 0.06;
        let corner = |su: f32, sv: f32| {
            let mut q = [0f64; 3];
            q[a] = x as f64;
            q[u] = if su < 0.0 { (lo[u] - m(u)) as f64 } else { (hi[u] + m(u)) as f64 };
            q[v] = if sv < 0.0 { (lo[v] - m(v)) as f64 } else { (hi[v] + m(v)) as f64 };
            q
        };
        let corners = [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)];
        let mut center = [0f64; 3];
        center[a] = x as f64;
        center[u] = ((lo[u] + hi[u]) * 0.5) as f64;
        center[v] = ((lo[v] + hi[v]) * 0.5) as f64;
        let handle_id = id("poignée coupe");
        let hot = ui.anim(
            handle_id.with("survol"),
            if ui.hovered(handle_id) || ui.is_active(handle_id) { 1.0 } else { 0.0 },
            120.0,
        );
        layer.push(
            Node::custom(move |p, r| {
                let pts: Vec<Vec2> = corners.iter().filter_map(|q| to_screen(&cam, r, *q)).collect();
                if pts.len() == 4 {
                    p.dl.push_clip(r);
                    p.dl.quad([pts[0], pts[1], pts[2], pts[3]], hl.with_alpha(0.06), None);
                    for i in 0..4 {
                        p.dl.line(pts[i], pts[(i + 1) % 4], 1.5, hl.with_alpha(0.85));
                    }
                    p.dl.pop_clip();
                }
            })
            .fill(),
        );
        // Poignée : disque au centre du plan, glissée le long de la normale.
        let view = s.viewport.get();
        if let Some(q) = to_screen(&cam, view, center) {
            let size = 22.0;
            layer.push(
                Node::custom(move |p, r| {
                    let c0 = r.center();
                    let rad = 7.0 + 2.0 * hot;
                    let rr = Rect::from_xywh(c0.x - rad, c0.y - rad, 2.0 * rad, 2.0 * rad);
                    p.dl.shadow(rr, rad, 6.0, vec2(0.0, 1.0), Color::BLACK.with_alpha(0.3));
                    p.dl.rect(rr, rad, hl, Some((2.0, Color::WHITE)));
                })
                .id(handle_id)
                .cursor(Cursor::Grab)
                .tooltip(tr("Glisser pour déplacer le plan"))
                .size(Size::Px(size), Size::Px(size))
                .anchor(Anchor {
                    left: Some(q.x - view.min.x - size * 0.5),
                    top: Some(q.y - view.min.y - size * 0.5),
                    ..Default::default()
                }),
            );
        }
    }
    any.then_some(layer)
}

/// Glisser de la poignée du plan de coupe : le déplacement à l'écran est ramené sur la normale du plan.
pub fn handle(ui: &mut Ui, s: &mut AppState) {
    let handle_id = id("poignée coupe");
    if s.tool == Tool::Section
        && ui.dragging(handle_id).is_some()
        && !ui.press_started(handle_id)
        && let Some((lo, hi)) = s.scene_bounds
    {
        let a = s.section.axis.min(2);
        let span = (hi[a] - lo[a]).max(1e-9);
        let view = s.viewport.get();
        let mut c = [0f64; 3];
        for i in 0..3 {
            c[i] = ((lo[i] + hi[i]) * 0.5) as f64;
        }
        let mut e = c;
        e[a] += span as f64;
        if let (Some(p0), Some(p1)) = (to_screen(&s.camera, view, c), to_screen(&s.camera, view, e)) {
            let dir = p1 - p0;
            let len2 = dir.x * dir.x + dir.y * dir.y;
            if len2 > 1.0 {
                let d = ui.pointer_delta();
                let t = (d.x * dir.x + d.y * dir.y) / len2;
                s.section.position = (s.section.position + t).clamp(0.0, 1.0);
            }
        }
    }
    if s.tool == Tool::Measure && !ui.text_focused() && ui.input.keys.contains(&obv_ui::Key::Delete) {
        s.measure = MeasureState::default();
    }
}
