//! Sélection au pixel : bonne géométrie, bon triangle, point de surface exact.

use obv_gpu::{Gpu, GpuDesc, RenderTarget};
use obv_render::{Camera, MeshInput, SceneRenderer};

fn quad(z: f32, half: f32) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
    let p = vec![[-half, -half, z], [half, -half, z], [half, half, z], [-half, half, z]];
    (p, vec![[0.0, 0.0, 1.0]; 4], vec![0, 1, 2, 0, 2, 3])
}

#[test]
fn selection_au_pixel() {
    let gpu = Gpu::new(&GpuDesc { validation: true, ..Default::default() }).unwrap();
    let mut r = SceneRenderer::new(&gpu, RenderTarget::COLOR_FORMAT, RenderTarget::DEPTH_FORMAT).unwrap();
    let (fp, fnrm, fi) = quad(0.25, 0.5);
    let (bp, bn, bi) = quad(-1.0, 1.0);
    r.set_meshes(&[
        MeshInput { positions: &bp, normals: &bn, indices: &bi, color: [255; 4], id: 3, edges: &[] },
        MeshInput { positions: &fp, normals: &fnrm, indices: &fi, color: [255; 4], id: 7, edges: &[] },
    ])
    .unwrap();
    let cam = Camera { target: [0.0; 3], distance: 5.0, yaw: 0.0, pitch: 0.0, ..Default::default() };
    let size = [800.0, 600.0];
    // Centre : le petit carré devant.
    let hit = r.pick(&cam, size, [399.5, 299.5]).unwrap().expect("rien sous le centre");
    assert_eq!(hit.id, 7);
    assert!((hit.position[2] - 0.25).abs() < 1e-3, "{:?}", hit.position);
    assert!(hit.position[0].abs() < 0.01 && hit.position[1].abs() < 0.01, "{:?}", hit.position);
    // À droite du petit carré : le grand derrière, triangle 0 ou 1.
    let hit = r.pick(&cam, size, [520.0, 300.0]).unwrap().expect("rien à droite");
    assert_eq!(hit.id, 3);
    assert!(hit.triangle < 2);
    assert!((hit.position[2] + 1.0).abs() < 1e-3, "{:?}", hit.position);
    // Coin : le vide.
    assert!(r.pick(&cam, size, [2.0, 2.0]).unwrap().is_none());
    assert_eq!(gpu.validation_errors(), 0);
}
