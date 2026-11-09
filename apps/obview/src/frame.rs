//! Une image complète : fond et scène 3D dans la vue, interface par-dessus, même passe de rendu.

use std::cell::RefCell;

use obv_gpu::{Gpu, RenderTarget, Result};
use obv_io::Model;
use obv_render::{Band, MeshInput, SceneRenderer, feature_edges};
use obv_ui::{Theme, ThemeKind, Ui, gpu::UiRenderer};
use rayon::prelude::*;

use crate::{
    inspector::{self, Inspector},
    shell::AppState,
};

/// Angle minimal entre faces voisines pour une arête vive.
const FEATURE_ANGLE_DEG: f32 = 30.0;

/// Arêtes vives de chaque pièce, calculées une fois par modèle (en parallèle).
pub struct EdgeCache(pub Vec<Vec<[f32; 3]>>);

impl EdgeCache {
    pub fn compute(model: &Model) -> Self {
        EdgeCache(
            model
                .parts
                .par_iter()
                .map(|p| feature_edges(&p.mesh.positions, &p.mesh.indices, FEATURE_ANGLE_DEG))
                .collect(),
        )
    }
}

/// Fond, puis la scène dans le rendu choisi : vue normale, bandes de l'inspecteur ou rendu isolé (D-15).
pub fn draw_viewport(
    scene: &SceneRenderer,
    cmd: obv_gpu::vk::CommandBuffer,
    state: &AppState,
    theme: &Theme,
    scale: f32,
) {
    let v = state.viewport.get();
    let rect = [v.min.x * scale, v.min.y * scale, v.max.x * scale, v.max.y * scale];
    let c = &theme.color;
    let cam = &state.camera;
    match state.inspector {
        Inspector::Off => {
            let top = [c.viewport_top.r, c.viewport_top.g, c.viewport_top.b, 1.0];
            let bottom = [c.viewport_bottom.r, c.viewport_bottom.g, c.viewport_bottom.b, 1.0];
            scene.draw(cmd, rect, cam, top, bottom);
        }
        Inspector::Bands => {
            let bg = inspector::background(theme.kind == ThemeKind::Dark);
            scene.draw_background(cmd, rect, bg, bg);
            for (m, band) in inspector::device_bands(v, state, scale) {
                scene.draw_scene(cmd, rect, cam, m, band);
            }
        }
        Inspector::Single(m) => {
            let bg = inspector::background(theme.kind == ThemeKind::Dark);
            scene.draw_background(cmd, rect, bg, bg);
            scene.draw_scene(cmd, rect, cam, m, Band::ALL);
        }
    }
}

const STEEL: [u8; 4] = [0xc9, 0xcc, 0xd1, 0xff];

/// Teinte de sélection : acier mêlé d'accent, pour garder la lecture des volumes.
fn selection_tint(accent: [u8; 4]) -> [u8; 4] {
    let m = |a: u8, b: u8| ((a as f32) * 0.45 + (b as f32) * 0.55).round() as u8;
    [m(STEEL[0], accent[0]), m(STEEL[1], accent[1]), m(STEEL[2], accent[2]), 255]
}

/// Ce qui change la géométrie envoyée : sélection, visibilité, accent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneKey {
    selected: Option<usize>,
    hidden: Vec<usize>,
    accent: [u8; 4],
    edges: bool,
}

impl SceneKey {
    pub fn new(state: &AppState, accent: [u8; 4], edges: bool) -> Self {
        let hidden = state.parts.iter().enumerate().filter(|(_, p)| !p.visible).map(|(i, _)| i).collect();
        SceneKey { selected: state.selected, hidden, accent, edges }
    }
}

/// Géométrie visible ; la pièce sélectionnée prend la couleur d'accent.
pub fn mesh_inputs<'a>(
    model: &'a Model,
    state: &AppState,
    accent: [u8; 4],
    edges: Option<&'a EdgeCache>,
) -> Vec<MeshInput<'a>> {
    model
        .parts
        .iter()
        .enumerate()
        .filter(|(i, _)| state.parts.get(*i).is_none_or(|p| p.visible))
        .map(|(i, p)| MeshInput {
            positions: &p.mesh.positions,
            normals: &p.mesh.normals,
            indices: &p.mesh.indices,
            color: if state.selected == Some(i) { selection_tint(accent) } else { STEEL },
            id: i as u32,
            edges: edges.and_then(|e| e.0.get(i)).map(Vec::as_slice).unwrap_or(&[]),
        })
        .collect()
}

pub struct OffscreenFrame {
    pub target: RenderTarget,
    scene: SceneRenderer,
    ui: UiRenderer,
    edges: Option<EdgeCache>,
}

impl OffscreenFrame {
    pub fn new(gpu: &Gpu, width: u32, height: u32) -> Result<Self> {
        Ok(OffscreenFrame {
            target: RenderTarget::new(gpu, width, height)?,
            scene: SceneRenderer::new(gpu, RenderTarget::COLOR_FORMAT, RenderTarget::DEPTH_FORMAT)?,
            ui: UiRenderer::new(gpu, RenderTarget::COLOR_FORMAT, Some(RenderTarget::DEPTH_FORMAT), 2048, 1)?,
            edges: None,
        })
    }

    /// Envoie la géométrie visible ; la pièce sélectionnée prend la couleur d'accent.
    pub fn set_model(&mut self, model: &Model, state: &AppState, accent: [u8; 4]) -> Result<()> {
        if state.inspector != Inspector::Off && self.edges.is_none() {
            self.edges = Some(EdgeCache::compute(model));
        }
        let meshes = mesh_inputs(model, state, accent, self.edges.as_ref());
        self.scene.set_meshes(&meshes)
    }

    /// Rend l'image (l'interface doit déjà être terminée : `ui.finish`) et la relit en RGBA.
    pub fn render(&mut self, ui: &mut Ui, state: &AppState) -> Result<Vec<u8>> {
        let scale = ui.dl.scale;
        let c = &ui.theme.color;
        let bg = [c.app_bg.r, c.app_bg.g, c.app_bg.b, 1.0];
        let extent = self.target.extent();
        let renderer = RefCell::new(&mut self.ui);
        let scene = &self.scene;
        let dl = &ui.dl;
        let theme = &ui.theme;
        let atlas = RefCell::new(&mut ui.atlas);
        let mut prepare_result = Ok(());
        self.target.render_with(
            bg,
            |cmd| prepare_result = renderer.borrow_mut().prepare(cmd, 0, dl, &mut atlas.borrow_mut()),
            |cmd| {
                draw_viewport(scene, cmd, state, theme, scale);
                renderer.borrow().draw(cmd, extent);
            },
        )?;
        prepare_result?;
        Ok(self.target.read_rgba8())
    }
}
