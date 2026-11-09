//! Sélection au pixel : bonne géométrie, bon triangle, point de surface exact.

use obv_gpu::{Gpu, GpuDesc, RenderTarget};
use obv_render::{Camera, MeshInput, SceneParams, SceneRenderer};

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

    // Orthographique : même point au centre, point du grand carré à l'abscisse attendue.
    let ortho = Camera { ortho: true, radius: 1.5, ..cam };
    let hit = r.pick(&ortho, size, [399.5, 299.5]).unwrap().expect("orthographique : rien au centre");
    assert_eq!(hit.id, 7);
    assert!((hit.position[2] - 0.25).abs() < 1e-3, "{:?}", hit.position);
    let h = 5.0 * (35f64.to_radians() * 0.5).tan();
    let x = (520.5 / 800.0 * 2.0 - 1.0) * h * 800.0 / 600.0;
    let hit = r.pick(&ortho, size, [520.0, 300.0]).unwrap().expect("orthographique : rien à droite");
    assert_eq!(hit.id, 3);
    assert!((hit.position[0] - x).abs() < 2e-3 && (hit.position[2] + 1.0).abs() < 1e-3, "{:?}", hit.position);

    // Vue éclatée : le petit carré décalé de +1 en x n'est plus au centre.
    let mut offsets = vec![[0.0; 3]; 8];
    offsets[7] = [1.0, 0.0, 0.0];
    r.set_params(&cam, &SceneParams { offsets: &offsets, ..Default::default() }).unwrap();
    assert_eq!(r.pick(&cam, size, [399.5, 299.5]).unwrap().unwrap().id, 3);

    // Coupe z <= 0 : le petit carré (z = 0,25) est retiré.
    r.set_params(&cam, &SceneParams { section: Some([0.0, 0.0, 1.0, 0.0]), offsets: &[] }).unwrap();
    assert_eq!(r.pick(&cam, size, [399.5, 299.5]).unwrap().unwrap().id, 3);
    assert_eq!(gpu.validation_errors(), 0);
}
