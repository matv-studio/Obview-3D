//! Navigation 3D : gestes du schéma choisi (D-13), molette, vues prédéfinies, transitions animées.
//!
//! L'orbite tourne autour du point de la surface sous le pointeur au début du geste (sinon autour de la cible),
//! le zoom garde fixe le point sous le pointeur, le panoramique suit le pointeur à la profondeur visée.

use obv_render::Camera;
use obv_ui::{FrameInput, Key, MouseButton, Rect, Vec2};

use crate::settings::{NavAction, Settings, resolve};

/// Radians par pixel logique pour l'orbite.
const ORBIT_SPEED: f64 = 0.0075;
/// Facteur de distance par cran de molette (avant la vitesse choisie).
const WHEEL_STEP: f64 = 0.16;
/// Pixels logiques par cran de molette (voir la conversion dans la fenêtre).
pub const WHEEL_NOTCH_PX: f32 = 48.0;
const PITCH_LIMIT: f64 = 89.9 * std::f64::consts::PI / 180.0;

type V3 = [f64; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Repère de la caméra : droite, haut, arrière (de la cible vers l'œil), tous unitaires.
pub fn basis(yaw: f64, pitch: f64) -> (V3, V3, V3) {
    let (cy, sy, cp, sp) = (yaw.cos(), yaw.sin(), pitch.cos(), pitch.sin());
    let back = [cp * sy, sp, cp * cy];
    let right = [cy, 0.0, -sy];
    let up = [-sp * sy, cp, -sp * cy];
    (right, up, back)
}

/// Point du rayon passant par `p` (pixels logiques dans `view`) situé à la distance de vue `depth`.
pub fn point_on_ray(cam: &Camera, view: Rect, p: Vec2, depth: f64) -> V3 {
    let (right, up, back) = basis(cam.yaw, cam.pitch);
    let half_h = (cam.fov_y * 0.5).tan();
    let aspect = (view.width() / view.height().max(1.0)) as f64;
    let nx = ((p.x - view.min.x) / view.width().max(1.0)) as f64 * 2.0 - 1.0;
    let ny = 1.0 - ((p.y - view.min.y) / view.height().max(1.0)) as f64 * 2.0;
    let eye = cam.eye();
    let dir = add(add(mul(right, nx * half_h * aspect), mul(up, ny * half_h)), mul(back, -1.0));
    add(eye, mul(dir, depth))
}

/// Vues prédéfinies (pavé numérique façon Blender, cube de vue).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Front,
    Back,
    Right,
    Left,
    Top,
    Bottom,
    Iso,
}

impl View {
    pub fn angles(self) -> (f64, f64) {
        use std::f64::consts::{FRAC_PI_2, PI};
        match self {
            View::Front => (0.0, 0.0),
            View::Back => (PI, 0.0),
            View::Right => (FRAC_PI_2, 0.0),
            View::Left => (-FRAC_PI_2, 0.0),
            View::Top => (0.0, PITCH_LIMIT),
            View::Bottom => (0.0, -PITCH_LIMIT),
            View::Iso => (0.6, 0.45),
        }
    }
}

struct Drag {
    action: NavAction,
    button: MouseButton,
    last: Vec2,
    pivot: V3,
    /// Profondeur de vue du point saisi (panoramique sous le pointeur).
    depth: f64,
}

struct Transition {
    from: Camera,
    to: Camera,
    start: f64,
    duration: f64,
}

/// Contrôleur de navigation : reçoit les entrées de l'image, déplace la caméra.
#[derive(Default)]
pub struct Navigator {
    drag: Option<Drag>,
    prev_buttons: [bool; 3],
    transition: Option<Transition>,
}

/// Ce que la vue 3D sait de l'image : rectangle, pointeur au-dessus de l'interface ou non.
pub struct NavContext<'a> {
    pub input: &'a FrameInput,
    pub view: Rect,
    /// Le pointeur est sur un élément d'interface (ou une fenêtre modale est ouverte).
    pub over_ui: bool,
    pub settings: &'a Settings,
}

impl Navigator {
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
    pub fn animating(&self) -> bool {
        self.transition.is_some()
    }

    /// Lance une transition douce vers `to` (vues prédéfinies, cadrage).
    pub fn animate_to(&mut self, cam: &Camera, mut to: Camera, now: f64) {
        // Chemin le plus court autour de l'axe vertical.
        let tau = std::f64::consts::TAU;
        to.yaw = cam.yaw + ((to.yaw - cam.yaw + std::f64::consts::PI).rem_euclid(tau) - std::f64::consts::PI);
        self.transition = Some(Transition { from: *cam, to, start: now, duration: 0.28 });
    }

    pub fn set_view(&mut self, cam: &Camera, v: View, now: f64) {
        let (yaw, pitch) = v.angles();
        self.animate_to(cam, Camera { yaw, pitch, ..*cam }, now);
    }

    /// Traite une image. `pick` renvoie le point de la surface sous un pixel (pixels logiques), s'il y en a un.
    /// Renvoie vrai si la caméra a bougé.
    pub fn update(&mut self, ctx: &NavContext<'_>, cam: &mut Camera, pick: &mut dyn FnMut(Vec2) -> Option<V3>) -> bool {
        let input = ctx.input;
        let s = ctx.settings;
        let mut moved = false;
        if let Some(t) = &self.transition {
            let k = ((input.time - t.start) / t.duration).clamp(0.0, 1.0);
            let e = 1.0 - (1.0 - k).powi(3);
            let lerp = |a: f64, b: f64| a + (b - a) * e;
            cam.yaw = lerp(t.from.yaw, t.to.yaw);
            cam.pitch = lerp(t.from.pitch, t.to.pitch);
            // Distance interpolée en logarithme : vitesse perçue constante.
            cam.distance = (lerp(t.from.distance.ln(), t.to.distance.ln())).exp();
            for i in 0..3 {
                cam.target[i] = lerp(t.from.target[i], t.to.target[i]);
            }
            moved = true;
            if k >= 1.0 {
                self.transition = None;
            }
        }

        let pressed = |i: usize| input.buttons[i] && !self.prev_buttons[i];
        let buttons = [MouseButton::Left, MouseButton::Middle, MouseButton::Right];
        if self.drag.is_none()
            && let Some(p) = input.pointer
            && ctx.view.contains(p)
            && !ctx.over_ui
        {
            for (i, b) in buttons.iter().enumerate() {
                if pressed(i)
                    && let Some(action) = resolve(s.navigation, *b, input.modifiers, &input.held)
                {
                    self.transition = None;
                    let hit = if s.orbit_around_cursor || action == NavAction::Pan { pick(p) } else { None };
                    let (_, _, back) = basis(cam.yaw, cam.pitch);
                    let eye = cam.eye();
                    let (pivot, depth) = match (action, hit) {
                        (NavAction::Orbit, Some(h)) if s.orbit_around_cursor => (h, dot(sub(eye, h), back)),
                        (_, Some(h)) => (cam.target, dot(sub(eye, h), back)),
                        _ => (cam.target, cam.distance),
                    };
                    self.drag = Some(Drag { action, button: *b, last: p, pivot, depth: depth.max(1e-9) });
                    break;
                }
            }
        }
        if let Some(d) = &self.drag
            && !input.buttons[d.button as usize]
        {
            self.drag = None;
        }
        if let (Some(d), Some(p)) = (&mut self.drag, input.pointer) {
            let delta = p - d.last;
            d.last = p;
            if delta.x != 0.0 || delta.y != 0.0 {
                moved = true;
                match d.action {
                    NavAction::Orbit => {
                        orbit(cam, d.pivot, -delta.x as f64 * ORBIT_SPEED, delta.y as f64 * ORBIT_SPEED)
                    }
                    NavAction::Pan => {
                        let (right, up, _) = basis(cam.yaw, cam.pitch);
                        let wpp = 2.0 * d.depth * (cam.fov_y * 0.5).tan() / ctx.view.height().max(1.0) as f64;
                        let m = add(mul(right, -delta.x as f64 * wpp), mul(up, delta.y as f64 * wpp));
                        cam.target = add(cam.target, m);
                    }
                    NavAction::Zoom => {
                        let mut k = (delta.y as f64 * 0.006 * s.zoom_speed as f64).exp();
                        if s.invert_zoom {
                            k = 1.0 / k;
                        }
                        cam.distance = (cam.distance * k).max(1e-9);
                    }
                }
            }
        }

        // Molette : zoom vers le point sous le pointeur.
        if input.scroll.y != 0.0
            && self.drag.is_none()
            && let Some(p) = input.pointer
            && ctx.view.contains(p)
            && !ctx.over_ui
        {
            self.transition = None;
            let steps = (input.scroll.y / WHEEL_NOTCH_PX) as f64;
            let mut k = (-steps * WHEEL_STEP * s.zoom_speed as f64).exp();
            if s.invert_zoom {
                k = 1.0 / k;
            }
            let anchor = if s.zoom_to_cursor {
                pick(p).unwrap_or_else(|| point_on_ray(cam, ctx.view, p, cam.distance))
            } else {
                cam.target
            };
            scale_about(cam, anchor, k);
            moved = true;
        }

        // Pavé numérique (Blender) : 1 face, 3 droite, 7 dessus, Ctrl pour la vue opposée ; 0 vue isométrique.
        let ctrl = input.modifiers.ctrl;
        for k in &input.keys {
            let v = match k {
                Key::Numpad(1) => Some(if ctrl { View::Back } else { View::Front }),
                Key::Numpad(3) => Some(if ctrl { View::Left } else { View::Right }),
                Key::Numpad(7) => Some(if ctrl { View::Bottom } else { View::Top }),
                Key::Numpad(0) => Some(View::Iso),
                _ => None,
            };
            if let Some(v) = v {
                self.set_view(cam, v, input.time);
                moved = true;
            }
        }
        self.prev_buttons = input.buttons;
        moved
    }
}

/// Tourne la caméra autour de `pivot` : `dyaw` autour de la verticale, `dpitch` autour de l'axe droit.
pub fn orbit(cam: &mut Camera, pivot: V3, dyaw: f64, dpitch: f64) {
    let new_pitch = (cam.pitch + dpitch).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    let new_yaw = cam.yaw + dyaw;
    let (r0, u0, b0) = basis(cam.yaw, cam.pitch);
    let (r1, u1, b1) = basis(new_yaw, new_pitch);
    // Coordonnées de la cible dans l'ancien repère, recomposées dans le nouveau.
    let rel = sub(cam.target, pivot);
    let (x, y, z) = (dot(rel, r0), dot(rel, u0), dot(rel, b0));
    cam.target = add(pivot, add(add(mul(r1, x), mul(u1, y)), mul(b1, z)));
    cam.yaw = new_yaw;
    cam.pitch = new_pitch;
}

/// Rapproche (k < 1) ou éloigne la caméra en gardant `anchor` fixe à l'écran.
pub fn scale_about(cam: &mut Camera, anchor: V3, k: f64) {
    let k = k.clamp(1e-3, 1e3);
    if cam.distance * k < 1e-9 {
        return;
    }
    cam.target = add(anchor, mul(sub(cam.target, anchor), k));
    cam.distance *= k;
}

#[cfg(test)]
mod tests {
    use super::*;
    use obv_ui::{Modifiers, vec2};

    fn close(a: V3, b: V3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    #[test]
    fn repere_coherent_avec_la_camera() {
        let cam = Camera { yaw: 0.7, pitch: 0.3, ..Default::default() };
        let (_, _, back) = basis(cam.yaw, cam.pitch);
        let eye = cam.eye();
        let expect = mul(sub(eye, cam.target), 1.0 / cam.distance);
        assert!(close(back, expect));
    }

    #[test]
    fn orbite_autour_du_pivot_garde_sa_distance() {
        let mut cam = Camera { target: [1.0, 0.0, 0.0], distance: 4.0, ..Default::default() };
        let pivot = [0.0, 0.5, 0.0];
        let d0 = sub(cam.eye(), pivot);
        orbit(&mut cam, pivot, 0.4, -0.2);
        let d1 = sub(cam.eye(), pivot);
        assert!((dot(d0, d0) - dot(d1, d1)).abs() < 1e-9);
        assert!((cam.distance - 4.0).abs() < 1e-12);
    }

    #[test]
    fn zoom_garde_le_point_vise() {
        let view = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let mut cam = Camera::default();
        let p = vec2(600.0, 150.0);
        let anchor = point_on_ray(&cam, view, p, cam.distance * 0.8);
        scale_about(&mut cam, anchor, 0.5);
        // Le point reste sur le rayon du même pixel.
        let again = point_on_ray(&cam, view, p, {
            let (_, _, back) = basis(cam.yaw, cam.pitch);
            dot(sub(cam.eye(), anchor), back)
        });
        assert!(close(again, anchor));
    }

    #[test]
    fn geste_blender() {
        let s = Settings::default();
        let view = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let mut nav = Navigator::default();
        let mut cam = Camera::default();
        let yaw0 = cam.yaw;
        let step = |nav: &mut Navigator, cam: &mut Camera, x: f32, mid: bool, shift: bool| {
            let input = FrameInput {
                pointer: Some(vec2(x, 300.0)),
                buttons: [false, mid, false],
                modifiers: Modifiers { shift, ..Modifiers::NONE },
                ..Default::default()
            };
            nav.update(&NavContext { input: &input, view, over_ui: false, settings: &s }, cam, &mut |_| None)
        };
        step(&mut nav, &mut cam, 400.0, false, false);
        step(&mut nav, &mut cam, 400.0, true, false);
        assert!(step(&mut nav, &mut cam, 450.0, true, false));
        assert!(cam.yaw < yaw0, "glisser à droite : la scène tourne vers la droite");
        step(&mut nav, &mut cam, 450.0, false, false);
        // Maj + milieu : panoramique, l'angle ne change plus.
        let yaw1 = cam.yaw;
        let t0 = cam.target;
        step(&mut nav, &mut cam, 450.0, true, true);
        step(&mut nav, &mut cam, 500.0, true, true);
        assert_eq!(cam.yaw, yaw1);
        assert!(!close(cam.target, t0));
    }
}
