//! Inspecteur des rendus à la manière de la visionneuse web Marmoset (D-15) : bandes diagonales parallèles, une
//! par rendu, séparées par de fins traits clairs, nom de chaque rendu écrit le long de son trait en haut ; un clic
//! sur une bande isole ce rendu (étiquette en haut à gauche, un clic dessus revient aux bandes).

use obv_render::{Band, RenderMode};
use obv_ui::{Anchor, Color, Edges, Id, Node, Rect, Size, TextStyle, Ui, Vec2, Weight, vec2};

use crate::shell::{AppState, group};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Inspector {
    #[default]
    Off,
    Bands,
    Single(RenderMode),
}

/// Inclinaison des traits par rapport à la verticale.
const TILT_DEG: f32 = 18.0;

pub fn label(m: RenderMode) -> &'static str {
    match m {
        RenderMode::Lit => "Éclairé",
        RenderMode::Color => "Couleur",
        RenderMode::Normals => "Normales",
        RenderMode::Topology => "Topologie",
        RenderMode::XRay => "Rayons X",
        RenderMode::Edges => "Arêtes",
    }
}

fn normal() -> Vec2 {
    let t = TILT_DEG.to_radians();
    vec2(t.cos(), t.sin())
}

/// Étendue des bandes le long de la normale, en pixels logiques : toute la largeur de la vue, mesurée à mi-hauteur.
/// Les bandes sont donc de largeur égale sur tout le canevas, quel que soit le modèle (D-15).
pub fn span(view: Rect, _s: &AppState) -> (f32, f32) {
    let n = normal();
    let yc = view.center().y;
    (view.min.x * n.x + yc * n.y, view.max.x * n.x + yc * n.y)
}

/// Limites des bandes le long de la normale : N + 1 valeurs, infinies aux extrémités.
fn bounds((ul, ur): (f32, f32)) -> Vec<f32> {
    let count = RenderMode::ALL.len();
    let mut b = vec![f32::NEG_INFINITY];
    for k in 1..count {
        b.push(ul + (ur - ul) * k as f32 / count as f32);
    }
    b.push(f32::INFINITY);
    b
}

/// Bandes en pixels de la cible (`view` en pixels logiques, `scale` pixels par unité logique).
pub fn device_bands(view: Rect, s: &AppState, scale: f32) -> Vec<(RenderMode, Band)> {
    let n = normal();
    let b = bounds(span(view, s));
    RenderMode::ALL
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let lo = if b[i].is_finite() { b[i] * scale } else { -1e30 };
            let hi = if b[i + 1].is_finite() { b[i + 1] * scale } else { 1e30 };
            (*m, Band { normal: [n.x, n.y], lo, hi })
        })
        .collect()
}

/// Rendu sous le point `p` (pixels logiques).
pub fn band_at(view: Rect, s: &AppState, p: Vec2) -> Option<RenderMode> {
    if !view.contains(p) {
        return None;
    }
    let n = normal();
    let u = p.x * n.x + p.y * n.y;
    let b = bounds(span(view, s));
    (0..RenderMode::ALL.len()).find(|&i| u >= b[i] && u < b[i + 1]).map(|i| RenderMode::ALL[i])
}

/// Fond uni des vues de rendu (couleur linéaire). Même gris dans les deux thèmes : les noms en blanc et les
/// rayons X (lueur additive) restent lisibles, comme dans la visionneuse Marmoset.
pub fn background(_dark: bool) -> [f32; 4] {
    let c = Color::hex(0x4d5056);
    [c.r, c.g, c.b, 1.0]
}

/// Calque de l'inspecteur au-dessus de la vue : traits, noms, étiquette du rendu isolé, statistiques.
pub fn overlay(ui: &mut Ui, s: &AppState) -> Option<Node> {
    if s.inspector == Inspector::Off {
        return None;
    }
    let mut layer = Node::stack().fill();
    if s.inspector == Inspector::Bands {
        let style = TextStyle::new(12.5, Weight::SemiBold);
        let view = s.viewport.get();
        let sp = span(view, s);
        // Zone du gizmo (en haut à droite) que les noms évitent.
        let gizmo = match s.settings.gizmo {
            crate::settings::GizmoStyle::None => 0.0,
            g => crate::gizmo::size(g) + 26.0,
        };
        layer.push(
            Node::custom(move |p, r| {
                let n = normal();
                // La mise en page de l'image courante peut décaler la vue : on suit son rectangle.
                let shift = (r.min.x - view.min.x) * n.x + (r.min.y - view.min.y) * n.y;
                let b = bounds((sp.0 + shift, sp.1 + shift));
                let d_up = vec2(n.y, -n.x);
                let angle = d_up.y.atan2(d_up.x);
                let x_at = |u: f32, y: f32| (u - y * n.y) / n.x;
                p.dl.push_clip(r);
                for u in &b[1..RenderMode::ALL.len()] {
                    let (a, c) = (vec2(x_at(*u, r.min.y), r.min.y), vec2(x_at(*u, r.max.y), r.max.y));
                    p.dl.line(a, c, 1.0, Color::WHITE.with_alpha(0.55));
                }
                p.dl.pop_clip();
                // Nom le long du trait gauche de chaque bande ; la première prend un trait virtuel, à la même
                // distance que les autres.
                let step = b[2] - b[1];
                for (k, m) in RenderMode::ALL.iter().enumerate() {
                    let u = if k == 0 { b[1] - step } else { b[k] };
                    let l = p.res.fonts.layout(label(*m), style, p.dl.scale);
                    let mut y_start = r.min.y + 18.0 + l.width * n.x;
                    if x_at(u, y_start) + l.height * 2.0 > r.max.x - gizmo {
                        y_start = r.min.y + gizmo + l.width * n.x;
                    }
                    let on_line = vec2(x_at(u, y_start), y_start);
                    let pivot = on_line + vec2(n.x, n.y) * (8.0 + l.baseline);
                    p.dl.set_rotation(pivot, angle);
                    let pos = vec2(pivot.x, pivot.y - l.baseline);
                    p.res.fonts.draw(p.dl, p.res.atlas, &l, pos, Color::WHITE.with_alpha(0.94));
                    p.dl.clear_rotation();
                }
            })
            .fill(),
        );
    }
    if let Inspector::Single(m) = s.inspector {
        let id = Id::new("inspecteur retour");
        let hover = ui.anim(id.with("survol"), if ui.hovered(id) { 1.0 } else { 0.0 }, 90.0);
        let bg = Color::hex(0x101114).with_alpha(0.55 + 0.2 * hover);
        layer.push(
            Node::row()
                .id(id)
                .tooltip_shortcut("Toutes les vues", "Échap")
                .anchor(Anchor { left: Some(14.0), top: Some(14.0), ..Default::default() })
                .height(Size::Px(30.0))
                .padding(Edges { left: 8.0, right: 12.0, top: 0.0, bottom: 0.0 })
                .gap(6.0)
                .radius(8.0)
                .bg(bg)
                .child(Node::icon("chevron-left", 15.0, Color::WHITE.with_alpha(0.85)))
                .child(Node::text(label(m), TextStyle::new(13.0, Weight::SemiBold), Color::WHITE)),
        );
    }
    let (tris, verts) =
        s.parts.iter().filter(|p| p.visible).fold((0, 0), |(t, v), p| (t + p.triangles, v + p.vertices));
    let stat = |icon: &'static str, value: String, what: &str| {
        Node::row()
            .gap(5.0)
            .child(Node::icon(icon, 12.0, Color::WHITE.with_alpha(0.7)))
            .child(Node::text(value, TextStyle::new(11.5, Weight::SemiBold), Color::WHITE.with_alpha(0.92)))
            .child(Node::text(what, TextStyle::new(11.5, Weight::Regular), Color::WHITE.with_alpha(0.7)))
    };
    layer.push(
        Node::row()
            .anchor(Anchor { left: Some(12.0), bottom: Some(12.0), ..Default::default() })
            .height(Size::Px(28.0))
            .padding(Edges::xy(10.0, 0.0))
            .radius(6.0)
            .bg(Color::hex(0x101114).with_alpha(0.55))
            .gap(14.0)
            .child(stat("triangle", group(tris), "triangles"))
            .child(stat("vertex", group(verts), "sommets")),
    );
    Some(layer)
}
