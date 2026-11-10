//! Inspecteur des rendus à la manière de la visionneuse web Marmoset (D-15) : bandes diagonales parallèles, une
//! par rendu, séparées par de fins traits clairs, nom de chaque rendu écrit le long de son trait en haut ; un clic
//! sur une bande isole ce rendu (étiquette en haut à gauche, un clic dessus revient aux bandes).

use obv_render::{Band, RenderMode};
use obv_ui::{Anchor, Color, Edges, Id, Node, Rect, Size, TextStyle, Ui, Vec2, Weight, tr, vec2};

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
    tr(match m {
        RenderMode::Lit => "Éclairé",
        RenderMode::Color => "Couleur",
        RenderMode::Normals => "Normales",
        RenderMode::Topology => "Topologie",
        RenderMode::XRay => "Rayons X",
        RenderMode::Edges => "Arêtes",
    })
}

fn normal() -> Vec2 {
    let t = TILT_DEG.to_radians();
    vec2(t.cos(), t.sin())
}

/// Étendue des bandes le long de la normale, en pixels logiques : toute la largeur de la vue, mesurée à mi-hauteur.
/// Les bandes sont donc de largeur égale sur tout le canevas, quel que soit le modèle (D-15).
pub fn span(view: Rect) -> (f32, f32) {
    let n = normal();
    let yc = view.center().y;
    (view.min.x * n.x + yc * n.y, view.max.x * n.x + yc * n.y)
}

/// Nombre de limites : vue normale | Éclairé | Couleur | … | Arêtes.
pub const CUTS: usize = RenderMode::ALL.len();

/// Marge hors champ (fraction de l'étendue) : une limite placée à `-marge` ou `1 + marge` ne touche aucun
/// pixel de la vue, traits inclinés compris.
pub fn margin(view: Rect) -> f32 {
    let n = normal();
    (view.height() * 0.5 * n.y + 4.0) / (view.width() * n.x).max(1.0)
}

/// Coordonnée réduite d'une limite sortie de la vue (à gauche : `-OUT`, à droite : `1 + OUT`) : proche de la vue,
/// pour que toutes les limites entrent ensemble.
const OUT: f32 = 0.2;

/// Limites visées pour l'état `i`, en coordonnée réduite : 0 à 1 dans la vue, `-OUT` sortie à gauche, `1 + OUT`
/// sortie à droite (indépendant de la taille de la vue, qui peut changer pendant une transition). Vue normale seule
/// (tout repoussé à droite), bandes égales (la vue normale sortie à gauche), ou un rendu seul (sa bande
/// occupe tout).
pub fn target_cuts(i: Inspector) -> [f32; CUTS] {
    let n = RenderMode::ALL.len() as f32;
    std::array::from_fn(|k| match i {
        Inspector::Off => 1.0 + OUT,
        Inspector::Bands if k == 0 => -OUT,
        Inspector::Bands => k as f32 / n,
        Inspector::Single(m) => {
            let idx = RenderMode::ALL.iter().position(|x| *x == m).unwrap_or(0);
            if k <= idx { -OUT } else { 1.0 + OUT }
        }
    })
}

/// Coordonnée réduite → fraction de l'étendue : [-OUT, 0] vers [-marge, 0], [0, 1] inchangé, [1, 1 + OUT] vers
/// [1, 1 + marge] (continue et croissante : l'ordre des limites est gardé).
pub fn place(cuts: &[f32; CUTS], margin: f32) -> [f32; CUTS] {
    let k = margin / OUT;
    cuts.map(|u| {
        if u < 0.0 {
            u * k
        } else if u > 1.0 {
            1.0 + (u - 1.0) * k
        } else {
            u
        }
    })
}

/// Régions de la vue le long de la normale, en pixels logiques : `None` pour la vue normale, puis un rendu
/// par bande ; les régions vides ou hors champ sont omises.
pub fn regions(view: Rect, cuts: &[f32; CUTS]) -> Vec<(Option<RenderMode>, f32, f32)> {
    let cuts = place(cuts, margin(view));
    let n = normal();
    let (ul, ur) = span(view);
    let u = |c: f32| ul + (ur - ul) * c;
    let (vis_lo, vis_hi) = (view.min.x * n.x + view.min.y * n.y, view.max.x * n.x + view.max.y * n.y);
    let mut out = Vec::new();
    for k in 0..=CUTS {
        let lo = if k == 0 { f32::NEG_INFINITY } else { u(cuts[k - 1]) };
        let hi = if k == CUTS { f32::INFINITY } else { u(cuts[k]) };
        if hi - lo < 0.01 || hi <= vis_lo || lo >= vis_hi {
            continue;
        }
        out.push((if k == 0 { None } else { Some(RenderMode::ALL[k - 1]) }, lo, hi));
    }
    out
}

/// Régions en pixels de la cible (`scale` pixels par unité logique).
pub fn device_regions(view: Rect, cuts: &[f32; CUTS], scale: f32) -> Vec<(Option<RenderMode>, Band)> {
    let n = normal();
    regions(view, cuts)
        .into_iter()
        .map(|(m, lo, hi)| {
            let lo = if lo.is_finite() { lo * scale } else { -1e30 };
            let hi = if hi.is_finite() { hi * scale } else { 1e30 };
            (m, Band { normal: [n.x, n.y], lo, hi })
        })
        .collect()
}

/// Rendu sous le point `p` (pixels logiques), bandes au repos.
pub fn band_at(view: Rect, s: &AppState, p: Vec2) -> Option<RenderMode> {
    if !view.contains(p) {
        return None;
    }
    let n = normal();
    let u = p.x * n.x + p.y * n.y;
    let cuts = target_cuts(s.inspector);
    regions(view, &cuts).into_iter().find(|(_, lo, hi)| u >= *lo && u < *hi).and_then(|(m, _, _)| m)
}

/// Fond uni des vues de rendu (couleur linéaire). Même gris dans les deux thèmes : les noms en blanc et les
/// rayons X (lueur additive) restent lisibles, comme dans la visionneuse Marmoset.
pub fn background(_dark: bool) -> [f32; 4] {
    let c = Color::hex(0x4d5056);
    [c.r, c.g, c.b, 1.0]
}

/// Calque de l'inspecteur au-dessus de la vue : traits, noms, étiquette du rendu isolé, statistiques.
pub fn overlay(ui: &mut Ui, s: &AppState) -> Option<Node> {
    let cuts = s.inspector_cuts;
    let shown = regions(s.viewport.get(), &cuts).iter().any(|(m, _, _)| m.is_some());
    if s.inspector == Inspector::Off && !shown {
        return None;
    }
    let mut layer = Node::stack().fill();
    // Traits et noms le long des limites animées : ils entrent avec les bandes et sortent avec elles.
    {
        let style = TextStyle::new(12.5, Weight::SemiBold);
        let view = s.viewport.get();
        let sp = span(view);
        let cuts = place(&cuts, margin(view));
        // Zone du gizmo (en haut à droite) que les noms évitent.
        let gizmo = match s.settings.gizmo {
            crate::settings::GizmoStyle::None => 0.0,
            g => crate::gizmo::size(g) + 26.0,
        };
        let names: Vec<&'static str> = RenderMode::ALL.iter().map(|m| label(*m)).collect();
        layer.push(
            Node::custom(move |p, r| {
                let n = normal();
                // La mise en page de l'image courante peut décaler la vue : on suit son rectangle.
                let shift = (r.min.x - view.min.x) * n.x + (r.min.y - view.min.y) * n.y;
                let u = |c: f32| sp.0 + shift + (sp.1 - sp.0) * c;
                let d_up = vec2(n.y, -n.x);
                let angle = d_up.y.atan2(d_up.x);
                let x_at = |u: f32, y: f32| (u - y * n.y) / n.x;
                let inside = |u: f32| x_at(u, r.max.y) > r.min.x + 0.5 && x_at(u, r.min.y) < r.max.x - 0.5;
                p.dl.push_clip(r);
                for c in &cuts {
                    let uc = u(*c);
                    if inside(uc) {
                        let (a, b) = (vec2(x_at(uc, r.min.y), r.min.y), vec2(x_at(uc, r.max.y), r.max.y));
                        p.dl.line(a, b, 1.0, Color::WHITE.with_alpha(0.55));
                    }
                }
                p.dl.pop_clip();
                // Nom le long du trait gauche de chaque bande ; la première, dont le trait gauche (limite avec
                // la vue normale) est sorti, prend un trait virtuel à la distance d'une bande. Le texte tourné
                // n'est pas découpé : un nom n'est écrit que si son trait est dans la vue.
                for (k, name) in names.iter().enumerate() {
                    let width = if k + 1 < CUTS { cuts[k + 1] - cuts[k] } else { 1.0 - cuts[k] };
                    if width < 0.02 {
                        continue;
                    }
                    let uk = if k == 0 && k + 2 < CUTS && cuts[0] < 0.0 {
                        u(cuts[1] - (cuts[2] - cuts[1]))
                    } else {
                        u(cuts[k])
                    };
                    let l = p.res.fonts.layout(name, style, p.dl.scale);
                    let mut y_start = r.min.y + 18.0 + l.width * n.x;
                    if x_at(uk, y_start) + l.height * 2.0 > r.max.x - gizmo {
                        y_start = r.min.y + gizmo + l.width * n.x;
                    }
                    let on_line = vec2(x_at(uk, y_start), y_start);
                    if on_line.x < r.min.x + l.height || on_line.x > r.max.x - l.height * 2.0 {
                        continue;
                    }
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
        let appear = ui.appear(id.with("apparition"), 160.0);
        let hover = ui.anim(id.with("survol"), if ui.hovered(id) { 1.0 } else { 0.0 }, 90.0);
        let bg = Color::hex(0x101114).with_alpha(0.55 + 0.2 * hover);
        layer.push(
            Node::row()
                .id(id)
                .tooltip_shortcut(tr("Toutes les vues"), tr("Échap"))
                .anchor(Anchor { left: Some(14.0 - (1.0 - appear) * 6.0), top: Some(14.0), ..Default::default() })
                .opacity(appear)
                .height(Size::Px(30.0))
                .padding(Edges { left: 8.0, right: 12.0, top: 0.0, bottom: 0.0 })
                .gap(6.0)
                .radius(8.0)
                .bg(bg)
                .child(Node::icon("chevron-left", 15.0, Color::WHITE.with_alpha(0.85)))
                .child(Node::text(label(m), TextStyle::new(13.0, Weight::SemiBold), Color::WHITE)),
        );
    }
    Some(layer)
}

/// Compteur triangles et sommets (pièces visibles) en bas à gauche de la vue : dans les vues de rendu, et quand
/// le panneau des pièces est masqué (son pied de liste porte sinon ces nombres).
pub fn stats(ui: &Ui, s: &AppState) -> Option<Node> {
    let rendering = s.inspector != Inspector::Off;
    if s.parts.is_empty() || (!rendering && s.left_open) {
        return None;
    }
    let c = &ui.theme.color;
    // Vues de rendu : pastille sombre sur les bandes ; vue normale : pastille flottante du thème.
    let (bg, border, value_c, label_c) = if rendering {
        (Color::hex(0x101114).with_alpha(0.55), None, Color::WHITE.with_alpha(0.92), Color::WHITE.with_alpha(0.7))
    } else {
        (c.floating.with_alpha(0.92), Some(c.border), c.text, c.text_secondary)
    };
    let (tris, verts) =
        s.parts.iter().filter(|p| p.visible).fold((0, 0), |(t, v), p| (t + p.triangles, v + p.vertices));
    let stat = |icon: &'static str, value: String, what: &str| {
        Node::row()
            .gap(5.0)
            .child(Node::icon(icon, 12.0, label_c))
            .child(Node::text(value, TextStyle::new(11.5, Weight::SemiBold), value_c))
            .child(Node::text(what, TextStyle::new(11.5, Weight::Regular), label_c))
    };
    let mut pill = Node::row()
        .anchor(Anchor { left: Some(12.0), bottom: Some(12.0), ..Default::default() })
        .height(Size::Px(28.0))
        .padding(Edges::xy(10.0, 0.0))
        .radius(6.0)
        .bg(bg)
        .gap(14.0)
        .child(stat("triangle", group(tris), tr("triangles")))
        .child(stat("vertex", group(verts), tr("sommets")));
    if let Some(b) = border {
        pill = pill.border(1.0, b);
    }
    Some(Node::stack().fill().child(pill))
}
