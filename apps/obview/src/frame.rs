//! Une image complète : fond et scène 3D dans la vue, interface par-dessus, même passe de rendu.

use std::cell::RefCell;

use obv_gpu::{Gpu, RenderTarget, Result};
use obv_io::Model;
use obv_render::{Band, MeshInput, RenderMode, SceneParams, SceneRenderer, feature_edges};
use obv_ui::{Theme, ThemeKind, Ui, gpu::UiRenderer};
use rayon::prelude::*;

use crate::{
    inspector::{self, Inspector},
    shell::AppState,
    tools,
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
    draw_view(scene, cmd, state, theme, state.viewport.get(), scale, false);
}

/// Comme [`draw_viewport`] pour une vue `v` (pixels logiques) quelconque ; `transparent` : sans fond.
pub fn draw_view(
    scene: &SceneRenderer,
    cmd: obv_gpu::vk::CommandBuffer,
    state: &AppState,
    theme: &Theme,
    v: obv_ui::Rect,
    scale: f32,
    transparent: bool,
) {
    let rect = [v.min.x * scale, v.min.y * scale, v.max.x * scale, v.max.y * scale];
    let c = &theme.color;
    let cam = &state.camera;
    let offsets = tools::explode_offsets(state);
    let params = SceneParams { section: tools::section_plane(state), offsets: &offsets };
    if let Err(e) = scene.set_params(cam, &params) {
        eprintln!("Obview 3D : paramètres de l'image : {e}");
    }
    let background = |top: [f32; 4], bottom: [f32; 4]| {
        if !transparent {
            scene.draw_background(cmd, rect, top, bottom);
        }
    };
    match state.inspector {
        Inspector::Off => {
            let top = [c.viewport_top.r, c.viewport_top.g, c.viewport_top.b, 1.0];
            let bottom = [c.viewport_bottom.r, c.viewport_bottom.g, c.viewport_bottom.b, 1.0];
            background(top, bottom);
            scene.draw_scene(cmd, rect, cam, RenderMode::Lit, Band::ALL);
        }
        Inspector::Bands => {
            let bg = inspector::background(theme.kind == ThemeKind::Dark);
            background(bg, bg);
            for (m, band) in inspector::device_bands(v, state, scale) {
                scene.draw_scene(cmd, rect, cam, m, band);
            }
        }
        Inspector::Single(m) => {
            let bg = inspector::background(theme.kind == ThemeKind::Dark);
            background(bg, bg);
            scene.draw_scene(cmd, rect, cam, m, Band::ALL);
        }
    }
}

/// Image de la vue 3D seule (sans interface) de `width` × `height` pixels, suréchantillonnée 2 × 2 quand la
/// taille le permet, en RGBA sRGB (alpha nul hors des pièces si `transparent`).
#[allow(clippy::too_many_arguments)]
pub fn capture(
    gpu: &Gpu,
    model: &Model,
    state: &AppState,
    theme: &Theme,
    accent: [u8; 4],
    width: u32,
    height: u32,
    transparent: bool,
) -> Result<Vec<u8>> {
    let ss = if width.max(height) * 2 <= 8192 { 2 } else { 1 };
    let (w, h) = (width * ss, height * ss);
    let target = RenderTarget::new(gpu, w, h)?;
    let mut scene = SceneRenderer::new(gpu, RenderTarget::COLOR_FORMAT, RenderTarget::DEPTH_FORMAT)?;
    let edges = (state.inspector != Inspector::Off).then(|| EdgeCache::compute(model));
    scene.set_meshes(&mesh_inputs(model, state, accent, edges.as_ref()))?;
    let v = state.viewport.get();
    let scale = w as f32 / v.width().max(1.0);
    let view = obv_ui::Rect::from_xywh(0.0, 0.0, v.width(), v.height());
    let clear = if transparent { [0.0; 4] } else { [0.0, 0.0, 0.0, 1.0] };
    target.render(clear, |cmd| draw_view(&scene, cmd, state, theme, view, scale, transparent))?;
    let rgba = target.read_rgba8();
    Ok(if ss == 1 { rgba } else { downsample(&rgba, w, h, ss) })
}

/// Moyenne de blocs `k` × `k` en lumière linéaire (alpha prémultiplié), retour en sRGB.
fn downsample(src: &[u8], w: u32, h: u32, k: u32) -> Vec<u8> {
    let to_lin: Vec<f32> = (0..256)
        .map(|i| {
            let c = i as f32 / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        })
        .collect();
    let (ow, oh) = (w / k, h / k);
    let mut out = vec![0u8; (ow * oh * 4) as usize];
    out.par_chunks_mut((ow * 4) as usize).enumerate().for_each(|(y, row)| {
        for x in 0..ow as usize {
            let mut acc = [0f32; 4];
            for dy in 0..k as usize {
                for dx in 0..k as usize {
                    let i = (((y * k as usize + dy) * w as usize) + x * k as usize + dx) * 4;
                    let a = src[i + 3] as f32 / 255.0;
                    for c in 0..3 {
                        acc[c] += to_lin[src[i + c] as usize] * a;
                    }
                    acc[3] += a;
                }
            }
            let n = (k * k) as f32;
            let a = acc[3] / n;
            for c in 0..3 {
                let lin = if a > 0.0 { acc[c] / n / a } else { 0.0 };
                let s = if lin <= 0.0031308 { lin * 12.92 } else { 1.055 * lin.powf(1.0 / 2.4) - 0.055 };
                row[x * 4 + c] = (s * 255.0).round().clamp(0.0, 255.0) as u8;
            }
            row[x * 4 + 3] = (a * 255.0).round() as u8;
        }
    });
    out
}

/// Couleur des pièces sans matériau.
pub const STEEL: [u8; 4] = [0xc9, 0xcc, 0xd1, 0xff];

/// Teinte de sélection : couleur de la pièce mêlée d'accent, pour garder la lecture des volumes.
fn selection_tint(base: [u8; 4], accent: [u8; 4]) -> [u8; 4] {
    let m = |a: u8, b: u8| ((a as f32) * 0.45 + (b as f32) * 0.55).round() as u8;
    [m(base[0], accent[0]), m(base[1], accent[1]), m(base[2], accent[2]), 255]
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
            color: {
                let base = model.display_color(i).unwrap_or(STEEL);
                if state.selected == Some(i) { selection_tint(base, accent) } else { base }
            },
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
