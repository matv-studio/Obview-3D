//! Inspecteur des rendus à la manière de la visionneuse web Marmoset (D-15, D-37) : bandes inclinées « //// »,
//! une par rendu (Normales, Albédo, Réflectivité, Brillance, Topologie par défaut, choisis dans les réglages),
//! la vue éclairée à droite de la dernière ; nom de chaque rendu le long de son trait, à gauche. Un clic sur une
//! bande isole ce rendu (étiquette en haut à gauche, un clic dessus revient aux bandes).

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

/// Inclinaison des traits de la visionneuse Marmoset : `x + 0,25 y` constant en coordonnées normalisées de la vue.
const SLANT: f32 = 0.25;

pub fn label(m: RenderMode) -> &'static str {
    tr(match m {
        RenderMode::Lit => "Éclairé",
        RenderMode::Albedo => "Albédo",
        RenderMode::Normals => "Normales",
        RenderMode::Reflectivity => "Réflectivité",
        RenderMode::Gloss => "Brillance",
        RenderMode::Topology => "Topologie",
        RenderMode::XRay => "Rayons X",
        RenderMode::Edges => "Arêtes",
    })
}

/// Info-bulle d'un rendu (réglages).
pub fn tooltip(m: RenderMode) -> &'static str {
    tr(match m {
        RenderMode::Lit => "Rendu éclairé",
        RenderMode::Albedo => "Couleur de base sans éclairage (métaux en noir)",
        RenderMode::Normals => "Orientation des surfaces",
        RenderMode::Reflectivity => "Réflexion à incidence normale",
        RenderMode::Gloss => "Inverse de la rugosité",
        RenderMode::Topology => "Arêtes des triangles",
        RenderMode::XRay => "Transparence additive",
        RenderMode::Edges => "Arêtes vives et contours",
    })
}

/// Normale des traits en pixels : l'inclinaison suit les proportions de la vue, comme dans la visionneuse
/// Marmoset (trait `x + 0,25 · (L / H) · y` constant, penché vers la droite en haut).
pub fn normal(view: Rect) -> Vec2 {
    let k = SLANT * view.width() / view.height().max(1.0);
    let l = (1.0 + k * k).sqrt();
    vec2(1.0 / l, k / l)
}

/// Étendue des bandes le long de la normale, en pixels logiques : toute la largeur de la vue, mesurée à mi-hauteur.
pub fn span(view: Rect) -> (f32, f32) {
    let n = normal(view);
    let yc = view.center().y;
    (view.min.x * n.x + yc * n.y, view.max.x * n.x + yc * n.y)
}

/// Une limite par rendu possible : la limite `k` borne à droite la bande de `RenderMode::VIEWS[k]` ; la vue
/// éclairée est à droite de la dernière.
pub const CUTS: usize = RenderMode::VIEWS.len();

/// Rendus affichés, dans l'ordre de `RenderMode::VIEWS`.
pub type Shown = [bool; CUTS];

/// Marge hors champ (fraction de l'étendue) : une limite placée à `-marge` ou `1 + marge` ne touche aucun
/// pixel de la vue, traits inclinés compris.
pub fn margin(view: Rect) -> f32 {
    let n = normal(view);
    (view.height() * 0.5 * n.y + 4.0) / (view.width() * n.x).max(1.0)
}

/// Coordonnée réduite d'une limite sortie de la vue (à gauche : `-OUT`, à droite : `1 + OUT`).
const OUT: f32 = 0.2;

/// Limites visées, en coordonnée réduite (0 à 1 dans la vue). Vue éclairée seule : tout sorti à gauche, comme la
/// visionneuse Marmoset ; bandes : `n` traits espacés de `e = 1 / (n − 1 + 8/3)`, le premier à `4/3 · e` du bord
/// (positions −0,6 … 0,6 de Marmoset pour cinq rendus) ; rendu isolé : les limites avant lui sorties à gauche,
/// la sienne et les suivantes à droite. Un rendu masqué a une bande vide (sa limite rejoint la précédente).
pub fn target_cuts(i: Inspector, shown: &Shown) -> [f32; CUTS] {
    let n = shown.iter().filter(|v| **v).count().max(1) as f32;
    let e = 1.0 / (n - 1.0 + 8.0 / 3.0);
    let mut out = [-OUT; CUTS];
    let mut prev = -OUT;
    let mut j = 0.0;
    for k in 0..CUTS {
        out[k] = match i {
            Inspector::Off => -OUT,
            Inspector::Bands if shown[k] => {
                prev = (4.0 / 3.0 + j) * e;
                j += 1.0;
                prev
            }
            Inspector::Bands => prev,
            Inspector::Single(m) => {
                let idx = RenderMode::VIEWS.iter().position(|x| *x == m).unwrap_or(0);
                if k < idx { -OUT } else { 1.0 + OUT }
            }
        };
    }
    out
}

/// Limites visées pour l'état de l'application.
pub fn targets(s: &AppState) -> [f32; CUTS] {
    target_cuts(s.inspector, &s.settings.shown_views())
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

/// Régions de la vue le long de la normale, en pixels logiques : un rendu par bande, puis `None` pour la vue
/// éclairée ; les régions vides ou hors champ sont omises.
pub fn regions(view: Rect, cuts: &[f32; CUTS]) -> Vec<(Option<RenderMode>, f32, f32)> {
    let cuts = place(cuts, margin(view));
    let n = normal(view);
    let (ul, ur) = span(view);
    let u = |c: f32| ul + (ur - ul) * c;
    let corners = [view.min, vec2(view.max.x, view.min.y), vec2(view.min.x, view.max.y), view.max];
    let dots = corners.map(|p| p.x * n.x + p.y * n.y);
    let vis_lo = dots.iter().copied().fold(f32::INFINITY, f32::min);
    let vis_hi = dots.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut out = Vec::new();
    for k in 0..=CUTS {
        let lo = if k == 0 { f32::NEG_INFINITY } else { u(cuts[k - 1]) };
        let hi = if k == CUTS { f32::INFINITY } else { u(cuts[k]) };
        if hi - lo < 0.01 || hi <= vis_lo || lo >= vis_hi {
            continue;
        }
        out.push((RenderMode::VIEWS.get(k).copied(), lo, hi));
    }
    out
}

/// Régions en pixels de la cible (`scale` pixels par unité logique).
pub fn device_regions(view: Rect, cuts: &[f32; CUTS], scale: f32) -> Vec<(Option<RenderMode>, Band)> {
    let n = normal(view);
    regions(view, cuts)
        .into_iter()
        .map(|(m, lo, hi)| {
            let lo = if lo.is_finite() { lo * scale } else { -1e30 };
            let hi = if hi.is_finite() { hi * scale } else { 1e30 };
            (m, Band { normal: [n.x, n.y], lo, hi })
        })
        .collect()
}

/// Rendu sous le point `p` (pixels logiques), bandes au repos ; `None` sur la vue éclairée.
pub fn band_at(view: Rect, s: &AppState, p: Vec2) -> Option<RenderMode> {
    if !view.contains(p) {
        return None;
    }
    let n = normal(view);
    let u = p.x * n.x + p.y * n.y;
    regions(view, &targets(s)).into_iter().find(|(_, lo, hi)| u >= *lo && u < *hi).and_then(|(m, _, _)| m)
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
        let n = normal(view);
        let sp = span(view);
        let cuts = place(&cuts, margin(view));
        // Zone du gizmo (en haut à droite) que les noms évitent.
        let gizmo = match s.settings.gizmo {
            crate::settings::GizmoStyle::None => 0.0,
            g => crate::gizmo::size(g) + 26.0,
        };
        let names: Vec<&'static str> = RenderMode::VIEWS.iter().map(|m| label(*m)).collect();
        layer.push(
            Node::custom(move |p, r| {
                // La mise en page de l'image courante peut décaler la vue : on suit son rectangle.
                let shift = (r.min.x - view.min.x) * n.x + (r.min.y - view.min.y) * n.y;
                let u = |c: f32| sp.0 + shift + (sp.1 - sp.0) * c;
                let d_up = vec2(n.y, -n.x);
                let angle = d_up.y.atan2(d_up.x);
                let x_at = |u: f32, y: f32| (u - y * n.y) / n.x;
                // Trait penché vers la droite en haut : visible si son haut est à droite du bord gauche et son bas à
                // gauche du bord droit.
                let inside = |u: f32| x_at(u, r.min.y) > r.min.x + 0.5 && x_at(u, r.max.y) < r.max.x - 0.5;
                p.dl.push_clip(r);
                for (k, c) in cuts.iter().enumerate() {
                    // Bande vide (rendu masqué) : même trait que la limite précédente, tracé une fois.
                    if k > 0 && (c - cuts[k - 1]).abs() < 1e-4 {
                        continue;
                    }
                    let uc = u(*c);
                    if inside(uc) {
                        let (a, b) = (vec2(x_at(uc, r.min.y), r.min.y), vec2(x_at(uc, r.max.y), r.max.y));
                        p.dl.line(a, b, 1.0, Color::WHITE.with_alpha(0.55));
                    }
                }
                p.dl.pop_clip();
                // Nom à gauche du trait qui borne la bande à droite, lu de bas en haut. Le texte tourné n'est
                // pas découpé : un nom n'est écrit que si son trait est dans la vue.
                for (k, name) in names.iter().enumerate() {
                    let width = if k == 0 { 1.0 } else { cuts[k] - cuts[k - 1] };
                    if width < 0.02 {
                        continue;
                    }
                    let uk = u(cuts[k]);
                    let l = p.res.fonts.layout(name, style, p.dl.scale);
                    let mut y_start = r.min.y + 18.0 + l.width * n.x;
                    if x_at(uk, y_start) + l.height > r.max.x - gizmo {
                        y_start = r.min.y + gizmo + l.width * n.x;
                    }
                    let on_line = vec2(x_at(uk, y_start), y_start);
                    if on_line.x < r.min.x + l.height * 2.0 || on_line.x > r.max.x - l.height {
                        continue;
                    }
                    let pivot = on_line - vec2(n.x, n.y) * 8.0;
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
    // Multivue (D-42) : sous la puce de la vue, la barre des outils occupant le bas des vues étroites.
    let at = if crate::multiview::visible(s) {
        Anchor { left: Some(10.0), top: Some(44.0), ..Default::default() }
    } else {
        Anchor { left: Some(12.0), bottom: Some(12.0), ..Default::default() }
    };
    let mut pill = Node::row()
        .anchor(at)
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

#[cfg(test)]
mod tests {
    use super::*;

    const ALL5: Shown = [true, true, true, true, true, false, false];

    #[test]
    fn positions_marmoset() {
        // Cinq rendus : traits à −0,6, −0,3, 0, 0,3, 0,6 en coordonnées normalisées, soit 0,2 … 0,8.
        let c = target_cuts(Inspector::Bands, &ALL5);
        for (k, want) in [0.2, 0.35, 0.5, 0.65, 0.8].iter().enumerate() {
            assert!((c[k] - want).abs() < 1e-5, "{k}: {}", c[k]);
        }
        // Rendus masqués : bande vide, limite égale à la précédente.
        assert_eq!(c[5], c[4]);
        assert_eq!(c[6], c[4]);
        // Vue éclairée seule : tout à gauche ; rendu isolé : sa bande couvre la vue.
        assert!(target_cuts(Inspector::Off, &ALL5).iter().all(|c| *c < 0.0));
        let s = target_cuts(Inspector::Single(RenderMode::Gloss), &ALL5);
        assert!(s[..3].iter().all(|c| *c < 0.0) && s[3..].iter().all(|c| *c > 1.0));
    }

    #[test]
    fn regions_dans_l_ordre() {
        let view = Rect::new(vec2(0.0, 0.0), vec2(1600.0, 900.0));
        let r = regions(view, &target_cuts(Inspector::Bands, &ALL5));
        let modes: Vec<_> = r.iter().map(|(m, _, _)| *m).collect();
        assert_eq!(
            modes,
            vec![
                Some(RenderMode::Normals),
                Some(RenderMode::Albedo),
                Some(RenderMode::Reflectivity),
                Some(RenderMode::Gloss),
                Some(RenderMode::Topology),
                None
            ]
        );
        // Penché vers la droite en haut : en haut, le point du premier trait est plus à droite qu'en bas.
        let n = normal(view);
        assert!(n.x > 0.0 && n.y > 0.0);
    }
}
