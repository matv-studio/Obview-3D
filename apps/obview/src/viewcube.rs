//! Cube de vue : orientation de la caméra, faces nommées. Un clic sur une face, une arête ou un coin oriente la
//! vue dans cette direction (26 vues, comme le ViewCube d'Autodesk) ; un glisser fait tourner la vue.

use obv_ui::{Color, Painter, Rect, TextStyle, Theme, Vec2, Weight, vec2};

/// Faces : normale (repère Y vers le haut), nom.
const FACES: [([f32; 3], &str); 6] = [
    ([0.0, 1.0, 0.0], "Dessus"),
    ([0.0, -1.0, 0.0], "Dessous"),
    ([0.0, 0.0, 1.0], "Avant"),
    ([0.0, 0.0, -1.0], "Arrière"),
    ([1.0, 0.0, 0.0], "Droite"),
    ([-1.0, 0.0, 0.0], "Gauche"),
];

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
    rect.width().min(rect.height()) * 0.30
}

/// Direction visée sous le pointeur `p` : composantes -1, 0 ou 1 (face, arête ou coin), ou rien hors du cube.
pub fn pick(rect: Rect, yaw: f32, pitch: f32, p: Vec2) -> Option<[f32; 3]> {
    let (eye, s, u) = frame(yaw, pitch);
    let k = scale(rect);
    let c = rect.center();
    let (x, y) = ((p.x - c.x) / k, -(p.y - c.y) / k);
    // Rayon orthographique venant de l'œil, intersection avec le cube [-1, 1]³ (méthode des dalles).
    let o: [f32; 3] = std::array::from_fn(|i| s[i] * x + u[i] * y + eye[i] * 10.0);
    let d = [-eye[0], -eye[1], -eye[2]];
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
    let dir = h.map(|v| {
        if v > 0.55 {
            1.0
        } else if v < -0.55 {
            -1.0
        } else {
            0.0
        }
    });
    (dir != [0.0; 3]).then_some(dir)
}

/// Angles de caméra (lacet, tangage) qui regardent le cube depuis `dir`.
pub fn angles(dir: [f32; 3], current_yaw: f64) -> (f64, f64) {
    let l = dot(dir, dir).sqrt().max(1e-6);
    let d = dir.map(|v| (v / l) as f64);
    let limit = 89.9f64.to_radians();
    let pitch = d[1].asin().clamp(-limit, limit);
    let yaw = if d[0].abs() < 1e-9 && d[2].abs() < 1e-9 { current_yaw } else { d[0].atan2(d[2]) };
    (yaw, pitch)
}

/// Dessine le cube pour une caméra d'angles `yaw`, `pitch` dans `rect` ; `hover` : direction survolée.
pub fn paint(p: &mut Painter<'_>, rect: Rect, yaw: f32, pitch: f32, hover: Option<[f32; 3]>, theme: &Theme) {
    let c = &theme.color;
    let (eye, s, u) = frame(yaw, pitch);
    let center = rect.center();
    let k = scale(rect);
    let project = |q: [f32; 3]| -> Vec2 { vec2(center.x + dot(q, s) * k, center.y - dot(q, u) * k) };

    // Faces visibles, de la plus fuyante à la plus frontale.
    let mut faces: Vec<(f32, [f32; 3], &str)> =
        FACES.iter().map(|(n, name)| (dot(*n, eye), *n, *name)).filter(|(d, _, _)| *d > 0.02).collect();
    faces.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (facing, n, name) in faces {
        // Deux axes du plan de la face.
        let t = if n[1].abs() > 0.5 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let b = cross(n, t);
        let corner = |a: f32, bb: f32| {
            project([n[0] + t[0] * a + b[0] * bb, n[1] + t[1] * a + b[1] * bb, n[2] + t[2] * a + b[2] * bb])
        };
        let pts = [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)];
        let shade = 0.55 + 0.45 * facing;
        let mut fill = c.floating.with_alpha(1.0).mix(c.raised_hover, 1.0 - shade).with_alpha(0.97);
        if hover == Some(n) {
            fill = fill.mix(c.accent, 0.35);
        }
        p.dl.quad(pts, fill, Some((1.0, c.border_strong)));
        if facing > 0.3 {
            let style = TextStyle::new(10.5, Weight::SemiBold);
            let layout = p.res.fonts.layout(name, style, p.dl.scale);
            let fc = project(n);
            let col = c.text_secondary.mul_alpha(((facing - 0.3) / 0.25).clamp(0.0, 1.0));
            p.res.fonts.draw(
                p.dl,
                p.res.atlas,
                &layout,
                vec2((fc.x - layout.width * 0.5).round(), (fc.y - layout.height * 0.5).round()),
                col,
            );
        }
    }

    // Arête ou coin survolé : pastille d'accent sur la surface du cube.
    if let Some(h) = hover
        && h.iter().filter(|v| **v != 0.0).count() > 1
    {
        let q = project(h.map(|v| v * 0.92));
        let r = obv_ui::Rect::from_xywh(q.x - 6.0, q.y - 6.0, 12.0, 12.0);
        p.dl.rect(r, 6.0, c.accent, Some((1.5, c.panel)));
    }

    // Trièdre des axes, en bas à gauche du cube.
    let origin = vec2(rect.min.x + 10.0, rect.max.y - 10.0);
    let axes: [([f32; 3], Color, &str); 3] =
        [([1.0, 0.0, 0.0], c.axis_x, "X"), ([0.0, 1.0, 0.0], c.axis_y, "Y"), ([0.0, 0.0, 1.0], c.axis_z, "Z")];
    for (a, col, name) in axes {
        let tip = vec2(origin.x + dot(a, s) * 16.0, origin.y - dot(a, u) * 16.0);
        p.dl.line(origin, tip, 1.75, col);
        let style = TextStyle::new(9.5, Weight::SemiBold);
        let layout = p.res.fonts.layout(name, style, p.dl.scale);
        let dir = vec2(dot(a, s), -dot(a, u));
        let lp = vec2(origin.x + dir.x * 22.0 - layout.width * 0.5, origin.y + dir.y * 22.0 - layout.height * 0.5);
        p.res.fonts.draw(p.dl, p.res.atlas, &layout, vec2(lp.x.round(), lp.y.round()), col);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_aretes_coins() {
        let r = Rect::from_xywh(0.0, 0.0, 100.0, 100.0);
        // Vue de face : le centre est la face avant, le bord droit l'arête avant-droite.
        assert_eq!(pick(r, 0.0, 0.0, vec2(50.0, 50.0)), Some([0.0, 0.0, 1.0]));
        assert_eq!(pick(r, 0.0, 0.0, vec2(78.0, 50.0)), Some([1.0, 0.0, 1.0]));
        assert_eq!(pick(r, 0.0, 0.0, vec2(78.0, 22.0)), Some([1.0, 1.0, 1.0]));
        assert_eq!(pick(r, 0.0, 0.0, vec2(2.0, 2.0)), None);
        let (yaw, pitch) = angles([1.0, 1.0, 1.0], 0.0);
        assert!((yaw - std::f64::consts::FRAC_PI_4).abs() < 1e-6);
        assert!((pitch - (1.0f64 / 3f64.sqrt()).asin()).abs() < 1e-6);
    }
}
