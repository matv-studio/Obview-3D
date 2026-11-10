//! Moteur OpenGL sans fenêtre (Linux, EGL) : un cube tracé et sélectionné au pixel.
#![cfg(target_os = "linux")]

use obv_gl::{GlScene, GlTarget, headless::Headless};
use obv_render::{Band, Camera, MeshInput, RenderMode, SceneParams};

fn cube() -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
    let mut p = Vec::new();
    let mut n = Vec::new();
    let mut i = Vec::new();
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            let base = p.len() as u32;
            let mut nn = [0.0; 3];
            nn[axis] = sign;
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let mut q = [0.0f32; 3];
                q[axis] = sign;
                q[u] = a;
                q[v] = b;
                p.push(q);
                n.push(nn);
            }
            if sign > 0.0 {
                i.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            } else {
                i.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
            }
        }
    }
    (p, n, i)
}

#[test]
fn cube_trace_et_selectionne() {
    let h = match Headless::new() {
        Ok(h) => h,
        Err(e) => {
            assert!(std::env::var_os("OBV_REQUIRE_GL").is_none(), "OpenGL sans fenêtre exigé (OBV_REQUIRE_GL) : {e}");
            eprintln!("OpenGL sans fenêtre indisponible : {e}");
            return;
        }
    };
    let dev = &h.device;
    eprintln!("{}, glClipControl : {}", dev.description, dev.clip_control);
    let (p, n, i) = cube();
    let mut scene = GlScene::new(dev).unwrap();
    scene
        .set_meshes(
            dev,
            &[MeshInput { positions: &p, normals: &n, indices: &i, color: [200, 60, 60, 255], id: 7, edges: &[] }],
        )
        .unwrap();
    let mut cam = Camera::default();
    cam.frame([-1.0; 3], [1.0; 3], 1.0);
    let target = GlTarget::new(dev, 64, 64).unwrap();
    target.begin(dev, [0.0, 0.0, 0.0, 1.0]);
    scene.set_params(dev, &cam, &SceneParams::default());
    let rect = [0.0, 0.0, 64.0, 64.0];
    scene.draw_background_band(dev, rect, 64, [0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 1.0, 1.0], Band::ALL);
    scene.draw_scene(dev, rect, 64, &cam, RenderMode::Color, Band::ALL);
    let img = target.read_rgba8(dev);
    let px = |x: usize, y: usize| &img[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
    // Centre : la couleur du cube ; coin : le fond bleu.
    assert_eq!(px(32, 32)[..3], [200, 60, 60], "{:?}", px(32, 32));
    assert!(px(1, 1)[2] > 200 && px(1, 1)[0] < 20, "{:?}", px(1, 1));
    let hit = scene.pick(dev, &cam, [64.0, 64.0], [32.0, 32.0]).expect("rien sous le centre");
    assert_eq!(hit.id, 7);
    let d = hit.position.iter().map(|v| v.abs()).fold(0.0, f64::max);
    assert!((d - 1.0).abs() < 1e-3, "point hors de la surface : {:?}", hit.position);
    assert!(scene.pick(dev, &cam, [64.0, 64.0], [1.0, 1.0]).is_none());
}
