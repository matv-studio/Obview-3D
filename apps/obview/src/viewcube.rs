//! Cube de vue : orientation de la caméra, faces nommées (un clic sur une face orientera la vue).

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

/// Dessine le cube pour une caméra d'angles `yaw`, `pitch` dans `rect`.
pub fn paint(p: &mut Painter<'_>, rect: Rect, yaw: f32, pitch: f32, theme: &Theme) {
    let c = &theme.color;
    // Direction de la caméra vers la cible, puis repère de l'écran.
    let eye = [pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos()];
    let f = [-eye[0], -eye[1], -eye[2]];
    let mut s = cross(f, [0.0, 1.0, 0.0]);
    let l = dot(s, s).sqrt().max(1e-6);
    s = [s[0] / l, s[1] / l, s[2] / l];
    let u = cross(s, f);
    let center = rect.center();
    let k = rect.width().min(rect.height()) * 0.30;
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
        let fill = c.floating.with_alpha(1.0).mix(c.raised_hover, 1.0 - shade).with_alpha(0.97);
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
