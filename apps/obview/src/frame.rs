//! Une image complète : fond et scène 3D dans la vue, interface par-dessus, même passe de rendu.

use std::cell::RefCell;

use obv_gpu::{Gpu, RenderTarget, Result};
use obv_io::Model;
use obv_render::{MeshInput, SceneRenderer};
use obv_ui::{Ui, gpu::UiRenderer};

use crate::shell::AppState;

const STEEL: [u8; 4] = [0xc9, 0xcc, 0xd1, 0xff];

/// Teinte de sélection : acier mêlé d'accent, pour garder la lecture des volumes.
fn selection_tint(accent: [u8; 4]) -> [u8; 4] {
    let m = |a: u8, b: u8| ((a as f32) * 0.45 + (b as f32) * 0.55).round() as u8;
    [m(STEEL[0], accent[0]), m(STEEL[1], accent[1]), m(STEEL[2], accent[2]), 255]
}

pub struct OffscreenFrame {
    pub target: RenderTarget,
    scene: SceneRenderer,
    ui: UiRenderer,
}

impl OffscreenFrame {
    pub fn new(gpu: &Gpu, width: u32, height: u32) -> Result<Self> {
        Ok(OffscreenFrame {
            target: RenderTarget::new(gpu, width, height)?,
            scene: SceneRenderer::new(gpu, RenderTarget::COLOR_FORMAT, RenderTarget::DEPTH_FORMAT)?,
            ui: UiRenderer::new(gpu, RenderTarget::COLOR_FORMAT, Some(RenderTarget::DEPTH_FORMAT), 2048, 1)?,
        })
    }

    /// Envoie la géométrie visible ; la pièce sélectionnée prend la couleur d'accent.
    pub fn set_model(&mut self, model: &Model, state: &AppState, accent: [u8; 4]) -> Result<()> {
        let meshes: Vec<MeshInput<'_>> = model
            .parts
            .iter()
            .enumerate()
            .filter(|(i, _)| state.parts.get(*i).is_none_or(|p| p.visible))
            .map(|(i, p)| MeshInput {
                positions: &p.mesh.positions,
                normals: &p.mesh.normals,
                indices: &p.mesh.indices,
                color: if state.selected == Some(i) { selection_tint(accent) } else { STEEL },
            })
            .collect();
        self.scene.set_meshes(&meshes)
    }

    /// Rend l'image (l'interface doit déjà être terminée : `ui.finish`) et la relit en RGBA.
    pub fn render(&mut self, ui: &mut Ui, state: &AppState) -> Result<Vec<u8>> {
        let scale = ui.dl.scale;
        let v = state.viewport.get();
        let rect = [v.min.x * scale, v.min.y * scale, v.max.x * scale, v.max.y * scale];
        let c = &ui.theme.color;
        let top = [c.viewport_top.r, c.viewport_top.g, c.viewport_top.b, 1.0];
        let bottom = [c.viewport_bottom.r, c.viewport_bottom.g, c.viewport_bottom.b, 1.0];
        let bg = [c.app_bg.r, c.app_bg.g, c.app_bg.b, 1.0];
        let extent = self.target.extent();
        let renderer = RefCell::new(&mut self.ui);
        let scene = &self.scene;
        let dl = &ui.dl;
        let atlas = RefCell::new(&mut ui.atlas);
        let mut prepare_result = Ok(());
        self.target.render_with(
            bg,
            |cmd| prepare_result = renderer.borrow_mut().prepare(cmd, 0, dl, &mut atlas.borrow_mut()),
            |cmd| {
                scene.draw(cmd, rect, &state.camera, top, bottom);
                renderer.borrow().draw(cmd, extent);
            },
        )?;
        prepare_result?;
        Ok(self.target.read_rgba8())
    }
}
