//! Gizmos de navigation (D-25), au choix dans les réglages :
//!
//! - **Axes** (Blender) : boules d'axe, pleines et lettrées côté positif, cerclées côté négatif ; disque au survol.
//! - **Cube** (ViewCube) : cube opaque à faces nommées, posées sur les faces par une transformation affine
//!   (projection orthographique) ; faces, arêtes et coins cliquables (26 vues) ; anneau de sol.
//! - **Trièdre** (CAO) : trois traits colorés lettrés, compact.
//!
//! Partout : un clic aligne la vue sur la direction visée, glisser fait tourner la vue.

use obv_ui::{Color, Painter, Rect, TextStyle, Theme, ThemeKind, Vec2, Weight, tr, vec2};

use crate::settings::GizmoStyle;

/// Rayon des boules d'axe (pixels logiques).
const BALL_R: f32 = 9.0;
/// Seuil des zones du cube : au-delà, la composante compte (arête ou coin).
const ZONE: f32 = 0.55;

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn add(a: [f32; 3], b: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}

/// Taille du carré occupé par le gizmo (pixels logiques).
pub fn size(style: GizmoStyle) -> f32 {
    match style {
        GizmoStyle::Axes => 112.0,
        GizmoStyle::Cube => 112.0,
        GizmoStyle::Triad => 84.0,
        GizmoStyle::None => 0.0,
    }
}

/// Repère de l'écran pour une caméra d'angles `yaw`, `pitch` : (œil, droite, haut).
fn frame(yaw: f32, pitch: f32) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let eye = [pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos()];
    let f = [-eye[0], -eye[1], -eye[2]];
    let mut s = cross(f, [0.0, 1.0, 0.0]);
    let l = dot(s, s).sqrt().max(1e-6);
    s = [s[0] / l, s[1] / l, s[2] / l];
    let u = cross(s, f);
    (eye, s, u)
}

struct View {
    eye: [f32; 3],
    s: [f32; 3],
    u: [f32; 3],
    center: Vec2,
    /// Pixels par unité du gizmo.
    k: f32,
}

impl View {
    fn new(style: GizmoStyle, rect: Rect, yaw: f32, pitch: f32) -> View {
        let (eye, s, u) = frame(yaw, pitch);
        let half = rect.width().min(rect.height()) * 0.5;
        let k = match style {
            GizmoStyle::Axes => half - BALL_R - 3.0,
            // Le coin du cube peut s'écarter de √3 unités ; anneau de sol à 1,5.
            GizmoStyle::Cube => (half - 3.0) / 1.78,
            GizmoStyle::Triad => half - 13.0,
            GizmoStyle::None => 1.0,
        };
        View { eye, s, u, center: rect.center(), k }
    }
    fn project(&self, q: [f32; 3]) -> Vec2 {
        vec2(self.center.x + dot(q, self.s) * self.k, self.center.y - dot(q, self.u) * self.k)
    }
}

const AXES: [[f32; 3]; 6] =
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]];

fn letter(a: [f32; 3]) -> &'static str {
    if a[0] != 0.0 {
        "X"
    } else if a[1] != 0.0 {
        "Y"
    } else {
        "Z"
    }
}

fn axis_color(theme: &Theme, a: [f32; 3]) -> Color {
    let c = &theme.color;
    if a[0] != 0.0 {
        c.axis_x
    } else if a[1] != 0.0 {
        c.axis_y
    } else {
        c.axis_z
    }
}

/// Nom d'une direction visée (info-bulle) : « Dessus », « Avant droite », « Dessus avant gauche »…
pub fn name(dir: [f32; 3]) -> String {
    let mut parts = Vec::new();
    if dir[1] > 0.0 {
        parts.push(tr("Dessus"));
    } else if dir[1] < 0.0 {
        parts.push(tr("Dessous"));
    }
    if dir[2] > 0.0 {
        parts.push(tr("Avant"));
    } else if dir[2] < 0.0 {
        parts.push(tr("Arrière"));
    }
    if dir[0] > 0.0 {
        parts.push(tr("Droite"));
    } else if dir[0] < 0.0 {
        parts.push(tr("Gauche"));
    }
    let s = parts.join(" ").to_lowercase();
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Direction visée sous le pointeur `p` : composantes -1, 0 ou 1 (axe, face, arête ou coin), ou rien.
pub fn pick(style: GizmoStyle, rect: Rect, yaw: f32, pitch: f32, p: Vec2) -> Option<[f32; 3]> {
    let v = View::new(style, rect, yaw, pitch);
    match style {
        GizmoStyle::Axes | GizmoStyle::Triad => {
            let (axes, r): (&[[f32; 3]], f32) =
                if style == GizmoStyle::Axes { (&AXES, BALL_R + 1.5) } else { (&AXES[..3], 10.0) };
            let mut best: Option<(f32, [f32; 3])> = None;
            for &a in axes {
                let d = p - v.project(a);
                if d.x * d.x + d.y * d.y <= r * r {
                    let depth = dot(a, v.eye);
                    if best.is_none_or(|(b, _)| depth > b) {
                        best = Some((depth, a));
                    }
                }
            }
            best.map(|(_, a)| a)
        }
        GizmoStyle::Cube => {
            // Rayon orthographique venant de l'œil (méthode des dalles).
            let (x, y) = ((p.x - v.center.x) / v.k, -(p.y - v.center.y) / v.k);
            let o: [f32; 3] = std::array::from_fn(|i| v.s[i] * x + v.u[i] * y + v.eye[i] * 10.0);
            let d = [-v.eye[0], -v.eye[1], -v.eye[2]];
            let (mut t0, mut t1) = (f32::MIN, f32::MAX);
            for i in 0..3 {
                if d[i].abs() < 1e-6 {
                    if o[i].abs() > 1.0 {
                        return None;
                    }
                    continue;
                }
                let (a, b) = ((-1.0 - o[i]) / d[i], (1.0 - o[i]) / d[i]);
                t0 = t0.max(a.min(b));
                t1 = t1.min(a.max(b));
            }
            if t0 > t1 {
                return None;
            }
            let h: [f32; 3] = std::array::from_fn(|i| o[i] + d[i] * t0);
            let dir = h.map(|c| {
                if c > ZONE {
                    1.0
                } else if c < -ZONE {
                    -1.0
                } else {
                    0.0
                }
            });
            (dir != [0.0; 3]).then_some(dir)
        }
        GizmoStyle::None => None,
    }
}

/// Angles de caméra (lacet, tangage) qui regardent le gizmo depuis `dir`.
pub fn angles(dir: [f32; 3], current_yaw: f64) -> (f64, f64) {
    let l = dot(dir, dir).sqrt().max(1e-6);
    let d = dir.map(|v| (v / l) as f64);
    let limit = 89.9f64.to_radians();
    let pitch = d[1].asin().clamp(-limit, limit);
    let yaw = if d[0].abs() < 1e-9 && d[2].abs() < 1e-9 { current_yaw } else { d[0].atan2(d[2]) };
    (yaw, pitch)
}

/// Couleur de survol : l'accent s'il y en a un, sinon le bleu « mode actif » de la charte.
fn hover_color(theme: &Theme) -> Color {
    let c = &theme.color;
    if c.has_accent { c.accent } else { c.set_text }
}

/// Dessine le gizmo. `hover` : direction survolée ; `active` : 0 → 1, le pointeur est sur le gizmo.
#[allow(clippy::too_many_arguments)]
pub fn paint(
    style: GizmoStyle,
    p: &mut Painter<'_>,
    rect: Rect,
    yaw: f32,
    pitch: f32,
    hover: Option<[f32; 3]>,
    active: f32,
    theme: &Theme,
) {
    let v = View::new(style, rect, yaw, pitch);
    match style {
        GizmoStyle::Axes => paint_axes(p, &v, rect, hover, active, theme),
        GizmoStyle::Cube => paint_cube(p, &v, hover, theme),
        GizmoStyle::Triad => paint_triad(p, &v, rect, hover, active, theme),
        GizmoStyle::None => {}
    }
}

fn ink(theme: &Theme) -> Color {
    if theme.kind == ThemeKind::Dark { Color::WHITE } else { Color::BLACK }
}

fn hover_disc(p: &mut Painter<'_>, rect: Rect, active: f32, theme: &Theme) {
    if active > 0.001 {
        let r = rect.width().min(rect.height()) * 0.5;
        let c = rect.center();
        let disc = Rect::from_xywh(c.x - r, c.y - r, 2.0 * r, 2.0 * r);
        p.dl.rect(disc, r, ink(theme).with_alpha(0.08 * active), None);
    }
}

fn centered_text(p: &mut Painter<'_>, text: &str, at: Vec2, style: TextStyle, color: Color) {
    let l = p.res.fonts.layout(text, style, p.dl.scale);
    let pos = vec2((at.x - l.width * 0.5).round(), (at.y - l.height * 0.5).round());
    p.res.fonts.draw(p.dl, p.res.atlas, &l, pos, color);
}

fn paint_axes(p: &mut Painter<'_>, v: &View, rect: Rect, hover: Option<[f32; 3]>, active: f32, theme: &Theme) {
    hover_disc(p, rect, active, theme);
    let mut order: Vec<[f32; 3]> = AXES.to_vec();
    order.sort_by(|a, b| dot(*a, v.eye).total_cmp(&dot(*b, v.eye)));
    let base = v.center;
    for a in order {
        let positive = a.iter().sum::<f32>() > 0.0;
        let col = axis_color(theme, a);
        let tip = v.project(a);
        let front = dot(a, v.eye) >= -0.05;
        let hovered = hover == Some(a);
        if positive {
            let d = tip - base;
            let len = (d.x * d.x + d.y * d.y).sqrt();
            if len > BALL_R + 1.0 {
                let end = base + d * ((len - BALL_R) / len);
                p.dl.line(base, end, 2.0, col.with_alpha(if front { 1.0 } else { 0.6 }));
            }
        }
        let r = if positive { BALL_R } else { BALL_R - 1.0 };
        let disc = Rect::from_xywh(tip.x - r, tip.y - r, 2.0 * r, 2.0 * r);
        if positive {
            let fill = if hovered { col.mix(Color::WHITE, 0.35) } else { col };
            p.dl.rect(disc, r, fill.with_alpha(if front { 1.0 } else { 0.8 }), None);
            centered_text(p, letter(a), tip, TextStyle::new(10.5, Weight::SemiBold), Color::hex(0x101010));
        } else {
            let fill = col.with_alpha(if hovered { 0.9 } else { 0.28 });
            p.dl.rect(disc, r, fill, Some((1.5, col.with_alpha(0.9))));
            if hovered {
                centered_text(
                    p,
                    &format!("-{}", letter(a)),
                    tip,
                    TextStyle::new(9.5, Weight::SemiBold),
                    Color::hex(0x101010),
                );
            }
        }
    }
}

fn paint_triad(p: &mut Painter<'_>, v: &View, rect: Rect, hover: Option<[f32; 3]>, active: f32, theme: &Theme) {
    hover_disc(p, rect, active, theme);
    let mut order: Vec<[f32; 3]> = AXES[..3].to_vec();
    order.sort_by(|a, b| dot(*a, v.eye).total_cmp(&dot(*b, v.eye)));
    let base = v.center;
    for a in order {
        let col = axis_color(theme, a);
        let tip = v.project(a);
        let hovered = hover == Some(a);
        let d = tip - base;
        let len = (d.x * d.x + d.y * d.y).sqrt();
        // Le trait s'arrête avant la lettre ; vu de bout, l'axe se réduit à sa lettre.
        if len > 9.0 {
            let end = base + d * ((len - 7.5) / len);
            p.dl.line(base, end, 2.0, col);
        }
        if hovered {
            let r = 9.0;
            p.dl.rect(Rect::from_xywh(tip.x - r, tip.y - r, 2.0 * r, 2.0 * r), r, col.with_alpha(0.25), None);
        }
        centered_text(p, letter(a), tip, TextStyle::new(11.5, Weight::SemiBold), col);
    }
    let r = 2.5;
    p.dl.rect(Rect::from_xywh(base.x - r, base.y - r, 2.0 * r, 2.0 * r), r, ink(theme).with_alpha(0.7), None);
}

/// Repère du texte sur chaque face : (normale, droite, haut), lisible depuis les vues usuelles.
/// Face du cube : normale, droite et haut du texte, nom.
type Face = ([f32; 3], [f32; 3], [f32; 3], &'static str);
const FACES: [Face; 6] = [
    ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], "Avant"),
    ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], "Arrière"),
    ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0], "Droite"),
    ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0], "Gauche"),
    ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0], "Dessus"),
    ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], "Dessous"),
];

fn paint_cube(p: &mut Painter<'_>, v: &View, hover: Option<[f32; 3]>, theme: &Theme) {
    let c = &theme.color;
    let dark = theme.kind == ThemeKind::Dark;
    let hl = hover_color(theme);

    let ring_color = if dark { Color::hex(0x4a4a4a) } else { Color::hex(0xc2c2c2) };
    // Anneau de sol (plan du dessous élargi) : moitié arrière sous le cube, moitié avant par-dessus.
    let ring = |p: &mut Painter<'_>, front: bool| {
        let n = 64;
        for i in 0..n {
            let (a0, a1) =
                (i as f32 / n as f32 * std::f32::consts::TAU, (i + 1) as f32 / n as f32 * std::f32::consts::TAU);
            let q0 = [1.5 * a0.cos(), -1.0, 1.5 * a0.sin()];
            let q1 = [1.5 * a1.cos(), -1.0, 1.5 * a1.sin()];
            let mid = [(q0[0] + q1[0]) * 0.5, -1.0, (q0[2] + q1[2]) * 0.5];
            // Devant le cube : le segment est plus près de l'œil que le centre de la face du dessous.
            let is_front = dot(mid, v.eye) > dot([0.0, -1.0, 0.0], v.eye);
            if is_front == front {
                // Opaque : des segments translucides se recouvrent aux jointures et paraissent pointillés.
                p.dl.line(v.project(q0), v.project(q1), 1.5, ring_color);
            }
        }
    };
    ring(p, false);

    let base = if dark { Color::hex(0x3a3a3a) } else { Color::hex(0xf4f4f4) };
    let edge = if dark { Color::hex(0x5c5c5c) } else { Color::hex(0xb8b8b8) };
    for (n, right, up, label) in FACES {
        let facing = dot(n, v.eye);
        if facing <= 0.0 {
            continue;
        }
        // Dessus et dessous : le texte prend, parmi les quatre sens de la face, celui qui monte le plus à
        // l'écran (lisible quelle que soit l'orbite) ; les côtés gardent la verticale.
        let (right, up) = if n[1] != 0.0 {
            let ups = [up, right, up.map(|x| -x), right.map(|x| -x)];
            let best = ups.into_iter().max_by(|a, b| dot(*a, v.u).total_cmp(&dot(*b, v.u))).unwrap_or(up);
            (cross(best, n), best)
        } else {
            (right, up)
        };
        let at = |a: f32, b: f32| v.project(add(add(n, right, a), up, b));
        let pts = [at(-1.0, -1.0), at(1.0, -1.0), at(1.0, 1.0), at(-1.0, 1.0)];
        // Éclairage doux venant du haut et de l'œil.
        let light = (0.55 * facing + 0.45 * (0.5 + 0.5 * n[1])).clamp(0.0, 1.0);
        let fill =
            if dark { base.mix(Color::WHITE, 0.10 * light) } else { base.mix(Color::BLACK, 0.10 * (1.0 - light)) };
        p.dl.quad(pts, fill, Some((1.0, edge)));

        // Zone survolée sur cette face : la composante le long de la normale doit être celle de la face.
        if let Some(h) = hover
            && dot(h, n) > 0.5
        {
            let range = |comp: f32| {
                if comp > 0.5 {
                    (ZONE, 1.0)
                } else if comp < -0.5 {
                    (-1.0, -ZONE)
                } else {
                    (-ZONE, ZONE)
                }
            };
            let (a0, a1) = range(dot(h, right));
            let (b0, b1) = range(dot(h, up));
            let zone = [at(a0, b0), at(a1, b0), at(a1, b1), at(a0, b1)];
            p.dl.quad(zone, hl.with_alpha(0.55), None);
        }

        // Nom posé sur la face : transformation affine (la projection est orthographique).
        if facing > 0.18 {
            let center = v.project(n);
            let rv = v.project(add(n, right, 1.0)) - center;
            let uv = v.project(add(n, up, 1.0)) - center;
            let k = v.k;
            let mut size = 11.0;
            let label = tr(label);
            let l = p.res.fonts.layout(label, TextStyle::new(size, Weight::SemiBold), p.dl.scale);
            let fit = 1.62 * k;
            if l.width > fit {
                size *= fit / l.width;
            }
            let l = p.res.fonts.layout(label, TextStyle::new(size, Weight::SemiBold), p.dl.scale);
            p.dl.set_transform(center, [rv.x / k, -uv.x / k, rv.y / k, -uv.y / k]);
            let pos = vec2(center.x - l.width * 0.5, center.y - l.height * 0.5);
            let alpha = ((facing - 0.18) / 0.35).clamp(0.0, 1.0);
            let col = if hover.is_some_and(|h| dot(h, n) > 0.5 && dot(h, right).abs() < 0.5 && dot(h, up).abs() < 0.5) {
                Color::WHITE
            } else {
                c.text.with_alpha(0.85)
            };
            p.res.fonts.draw(p.dl, p.res.atlas, &l, pos, col.with_alpha(col.a * alpha));
            p.dl.clear_rotation();
        }
    }
    ring(p, true);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_faces_aretes_coins() {
        let r = Rect::from_xywh(0.0, 0.0, 120.0, 120.0);
        let s = GizmoStyle::Cube;
        let v = View::new(s, r, 0.0, 0.0);
        assert_eq!(pick(s, r, 0.0, 0.0, vec2(60.0, 60.0)), Some([0.0, 0.0, 1.0]));
        let edge = v.project([0.8, 0.0, 1.0]);
        assert_eq!(pick(s, r, 0.0, 0.0, edge), Some([1.0, 0.0, 1.0]));
        let corner = v.project([0.8, 0.8, 1.0]);
        assert_eq!(pick(s, r, 0.0, 0.0, corner), Some([1.0, 1.0, 1.0]));
        assert_eq!(pick(s, r, 0.0, 0.0, vec2(1.0, 1.0)), None);
    }

    #[test]
    fn axes_et_triedre() {
        let r = Rect::from_xywh(0.0, 0.0, 112.0, 112.0);
        for s in [GizmoStyle::Axes, GizmoStyle::Triad] {
            let v = View::new(s, r, 0.3, 0.4);
            assert_eq!(pick(s, r, 0.3, 0.4, v.project([1.0, 0.0, 0.0])), Some([1.0, 0.0, 0.0]));
            assert_eq!(pick(s, r, 0.3, 0.4, v.project([0.0, 1.0, 0.0])), Some([0.0, 1.0, 0.0]));
        }
        let v = View::new(GizmoStyle::Axes, r, 0.3, 0.4);
        assert_eq!(pick(GizmoStyle::Axes, r, 0.3, 0.4, v.project([0.0, 0.0, -1.0])), Some([0.0, 0.0, -1.0]));
        let (yaw, pitch) = angles([1.0, 1.0, 1.0], 0.0);
        assert!((yaw - std::f64::consts::FRAC_PI_4).abs() < 1e-6);
        assert!((pitch - (1.0f64 / 3f64.sqrt()).asin()).abs() < 1e-6);
        obv_ui::i18n::set_lang(obv_ui::Lang::Fr);
        assert_eq!(name([0.0, 1.0, 0.0]), "Dessus");
        assert_eq!(name([1.0, 0.0, 1.0]), "Avant droite");
        assert_eq!(name([-1.0, 1.0, -1.0]), "Dessus arrière gauche");
        obv_ui::i18n::set_lang(obv_ui::Lang::En);
        crate::lang::init();
        assert_eq!(name([-1.0, 1.0, -1.0]), "Top back left");
    }
}
