//! Gizmo de navigation : cube semi-transparent traversé par les trois axes, à la manière de Blender.
//!
//! - Boules d'axe (pleines et lettrées côté positif, cerclées côté négatif) : un clic aligne la vue sur l'axe.
//! - Faces, arêtes et coins du cube : un clic oriente la vue dans cette direction (26 vues, comme le ViewCube).
//! - Glisser n'importe où sur le gizmo fait tourner la vue ; un disque discret apparaît au survol.

use obv_ui::{Color, Painter, Rect, TextStyle, Theme, Vec2, Weight, vec2};

/// Demi-côté du cube et distance des boules d'axe, en unités du gizmo.
const CUBE: f32 = 1.0;
const BALL_AT: f32 = 1.78;
/// Rayon des boules, en pixels logiques.
const BALL_R: f32 = 8.5;

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
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

fn scale(rect: Rect) -> f32 {
    (rect.width().min(rect.height()) * 0.5 - BALL_R - 2.0) / BALL_AT
}

struct View {
    eye: [f32; 3],
    s: [f32; 3],
    u: [f32; 3],
    center: Vec2,
    k: f32,
}

impl View {
    fn new(rect: Rect, yaw: f32, pitch: f32) -> View {
        let (eye, s, u) = frame(yaw, pitch);
        View { eye, s, u, center: rect.center(), k: scale(rect) }
    }
    fn project(&self, q: [f32; 3]) -> Vec2 {
        vec2(self.center.x + dot(q, self.s) * self.k, self.center.y - dot(q, self.u) * self.k)
    }
}

const AXES: [[f32; 3]; 6] =
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]];

/// Nom d'une direction visée (info-bulle) : « Dessus », « Avant droite », « Dessus avant gauche »…
pub fn name(dir: [f32; 3]) -> String {
    let mut parts = Vec::new();
    if dir[1] > 0.0 {
        parts.push("Dessus");
    } else if dir[1] < 0.0 {
        parts.push("Dessous");
    }
    if dir[2] > 0.0 {
        parts.push("Avant");
    } else if dir[2] < 0.0 {
        parts.push("Arrière");
    }
    if dir[0] > 0.0 {
        parts.push("Droite");
    } else if dir[0] < 0.0 {
        parts.push("Gauche");
    }
    let s = parts.join(" ").to_lowercase();
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Direction visée sous le pointeur `p` : composantes -1, 0 ou 1 (axe, face, arête ou coin), ou rien.
pub fn pick(rect: Rect, yaw: f32, pitch: f32, p: Vec2) -> Option<[f32; 3]> {
    let v = View::new(rect, yaw, pitch);
    // Boules d'axe d'abord, la plus proche de l'œil gagne.
    let mut best: Option<(f32, [f32; 3])> = None;
    for a in AXES {
        let q = v.project(a.map(|c| c * BALL_AT));
        let d = p - q;
        if d.x * d.x + d.y * d.y <= (BALL_R + 1.5) * (BALL_R + 1.5) {
            let depth = dot(a, v.eye);
            if best.is_none_or(|(b, _)| depth > b) {
                best = Some((depth, a));
            }
        }
    }
    if let Some((_, a)) = best {
        return Some(a);
    }
    // Puis le cube : rayon orthographique venant de l'œil (méthode des dalles).
    let (x, y) = ((p.x - v.center.x) / v.k, -(p.y - v.center.y) / v.k);
    let o: [f32; 3] = std::array::from_fn(|i| v.s[i] * x + v.u[i] * y + v.eye[i] * 10.0);
    let d = [-v.eye[0], -v.eye[1], -v.eye[2]];
    let (mut t0, mut t1) = (f32::MIN, f32::MAX);
    for i in 0..3 {
        if d[i].abs() < 1e-6 {
            if o[i].abs() > CUBE {
                return None;
            }
            continue;
        }
        let (a, b) = ((-CUBE - o[i]) / d[i], (CUBE - o[i]) / d[i]);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
    }
    if t0 > t1 {
        return None;
    }
    let h: [f32; 3] = std::array::from_fn(|i| o[i] + d[i] * t0);
    let dir = h.map(|c| {
        if c > 0.55 {
            1.0
        } else if c < -0.55 {
            -1.0
        } else {
            0.0
        }
    });
    (dir != [0.0; 3]).then_some(dir)
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

/// Dessine le gizmo pour une caméra d'angles `yaw`, `pitch` dans `rect`. `hover` : direction survolée ;
/// `active` : le pointeur est sur le gizmo (disque de fond).
pub fn paint(
    p: &mut Painter<'_>,
    rect: Rect,
    yaw: f32,
    pitch: f32,
    hover: Option<[f32; 3]>,
    active: f32,
    theme: &Theme,
) {
    let c = &theme.color;
    let v = View::new(rect, yaw, pitch);
    let dark = theme.kind == obv_ui::ThemeKind::Dark;
    let ink = if dark { Color::WHITE } else { Color::BLACK };

    // Disque de fond au survol.
    if active > 0.001 {
        let r = rect.width().min(rect.height()) * 0.5;
        let disc = Rect::from_xywh(v.center.x - r, v.center.y - r, 2.0 * r, 2.0 * r);
        p.dl.rect(disc, r, ink.with_alpha(0.07 * active), None);
    }

    let axis_color = |a: [f32; 3]| {
        if a[0] != 0.0 {
            c.axis_x
        } else if a[1] != 0.0 {
            c.axis_y
        } else {
            c.axis_z
        }
    };
    let letter = |a: [f32; 3]| {
        if a[0] != 0.0 {
            "X"
        } else if a[1] != 0.0 {
            "Y"
        } else {
            "Z"
        }
    };

    // Éléments triés de l'arrière vers l'avant : boules d'axe (avec leur trait) et faces du cube.
    enum Item {
        Axis([f32; 3]),
        Face([f32; 3]),
    }
    let mut items: Vec<(f32, Item)> = Vec::new();
    for a in AXES {
        items.push((dot(a, v.eye) * BALL_AT, Item::Axis(a)));
    }
    for n in AXES {
        items.push((dot(n, v.eye) * CUBE - 0.01, Item::Face(n)));
    }
    items.sort_by(|a, b| a.0.total_cmp(&b.0));

    for (depth, item) in items {
        match item {
            Item::Face(n) => {
                let facing = dot(n, v.eye);
                let t = if n[1].abs() > 0.5 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
                let b = cross(n, t);
                let corner = |a: f32, bb: f32| v.project(std::array::from_fn(|i| (n[i] + t[i] * a + b[i] * bb) * CUBE));
                let pts = [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)];
                let lit = 0.5 + 0.5 * facing.max(0.0);
                let mut fill = ink.with_alpha(if facing > 0.0 { 0.05 + 0.07 * lit } else { 0.025 });
                if hover == Some(n) {
                    fill = c.accent.with_alpha(0.42);
                }
                let edge = ink.with_alpha(if facing > 0.0 { 0.38 } else { 0.12 });
                p.dl.quad(pts, fill, Some((1.0, edge)));
            }
            Item::Axis(a) => {
                let positive = a.iter().sum::<f32>() > 0.0;
                let col = axis_color(a);
                let tip = v.project(a.map(|x| x * BALL_AT));
                let front = depth >= 0.0;
                if positive {
                    // Trait du centre à la boule, sous la boule.
                    let base = v.project([0.0; 3]);
                    let d = tip - base;
                    let len = (d.x * d.x + d.y * d.y).sqrt();
                    if len > BALL_R + 1.0 {
                        let end = base + d * ((len - BALL_R) / len);
                        p.dl.line(base, end, 2.0, col.with_alpha(if front { 0.95 } else { 0.55 }));
                    }
                }
                let r = BALL_R;
                let disc = Rect::from_xywh(tip.x - r, tip.y - r, 2.0 * r, 2.0 * r);
                let hovered = hover == Some(a);
                if positive {
                    let fill = if hovered { col.mix(Color::WHITE, 0.3) } else { col };
                    p.dl.rect(disc, r, fill.with_alpha(if front { 1.0 } else { 0.75 }), None);
                    let l = p.res.fonts.layout(letter(a), TextStyle::new(10.5, Weight::SemiBold), p.dl.scale);
                    let pos = vec2((tip.x - l.width * 0.5).round(), (tip.y - l.height * 0.5).round());
                    p.res.fonts.draw(p.dl, p.res.atlas, &l, pos, Color::hex(0x101010));
                } else {
                    let fill = col.with_alpha(if hovered { 0.85 } else { 0.22 });
                    p.dl.rect(disc, r, fill, Some((1.5, col.with_alpha(0.9))));
                    if hovered {
                        let l = p.res.fonts.layout(
                            &format!("-{}", letter(a)),
                            TextStyle::new(9.5, Weight::SemiBold),
                            p.dl.scale,
                        );
                        let pos = vec2((tip.x - l.width * 0.5).round(), (tip.y - l.height * 0.5).round());
                        p.res.fonts.draw(p.dl, p.res.atlas, &l, pos, Color::hex(0x101010));
                    }
                }
            }
        }
    }

    // Arête ou coin survolé : pastille d'accent sur la surface du cube.
    if let Some(h) = hover
        && h.iter().filter(|x| **x != 0.0).count() > 1
    {
        let q = v.project(h.map(|x| x * CUBE * 0.92));
        let r = Rect::from_xywh(q.x - 5.5, q.y - 5.5, 11.0, 11.0);
        p.dl.rect(r, 5.5, c.accent, Some((1.5, c.panel)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axes_faces_aretes_coins() {
        let r = Rect::from_xywh(0.0, 0.0, 120.0, 120.0);
        let v = View::new(r, 0.0, 0.0);
        // Vue de face : centre = face avant ; boule X à droite ; bord du cube = arête.
        assert_eq!(pick(r, 0.0, 0.0, vec2(60.0, 60.0)), Some([0.0, 0.0, 1.0]));
        let bx = v.project([BALL_AT, 0.0, 0.0]);
        assert_eq!(pick(r, 0.0, 0.0, bx), Some([1.0, 0.0, 0.0]));
        let edge = v.project([0.8, 0.0, 1.0]);
        assert_eq!(pick(r, 0.0, 0.0, edge), Some([1.0, 0.0, 1.0]));
        let corner = v.project([0.8, 0.8, 1.0]);
        assert_eq!(pick(r, 0.0, 0.0, corner), Some([1.0, 1.0, 1.0]));
        assert_eq!(pick(r, 0.0, 0.0, vec2(1.0, 1.0)), None);
        let (yaw, pitch) = angles([1.0, 1.0, 1.0], 0.0);
        assert!((yaw - std::f64::consts::FRAC_PI_4).abs() < 1e-6);
        assert!((pitch - (1.0f64 / 3f64.sqrt()).asin()).abs() < 1e-6);
        assert_eq!(name([0.0, 1.0, 0.0]), "Dessus");
        assert_eq!(name([1.0, 0.0, 1.0]), "Avant droite");
        assert_eq!(name([-1.0, 1.0, -1.0]), "Dessus arrière gauche");
    }
}
