//! Une image complète : fond et scène 3D dans la vue, interface par-dessus, même passe de rendu.

use std::cell::RefCell;

use obv_gl::{GlDevice, GlScene, GlTarget};
use obv_gpu::{Gpu, RenderTarget, Result, vk};
use obv_io::Model;
use obv_render::{Band, Camera, MeshInput, RenderMode, SceneParams, SceneRenderer, feature_edges};
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
/// Tracés de la scène, quel que soit le moteur (D-30) : paramètres de l'image, fond et surfaces par bande.
pub trait SceneDraw {
    fn set_params(&self, camera: &Camera, params: &SceneParams<'_>);
    /// `rect` : pixels de la cible, origine en haut à gauche.
    fn background(&self, rect: [f32; 4], top: [f32; 4], bottom: [f32; 4], band: Band);
    fn scene(&self, rect: [f32; 4], camera: &Camera, mode: RenderMode, band: Band);
}

/// Vulkan : tracés enregistrés dans `cmd`.
pub struct VkDraw<'a> {
    pub scene: &'a SceneRenderer,
    pub cmd: vk::CommandBuffer,
}

impl SceneDraw for VkDraw<'_> {
    fn set_params(&self, camera: &Camera, params: &SceneParams<'_>) {
        if let Err(e) = self.scene.set_params(camera, params) {
            eprintln!("Obview 3D : paramètres de l'image : {e}");
        }
    }
    fn background(&self, rect: [f32; 4], top: [f32; 4], bottom: [f32; 4], band: Band) {
        self.scene.draw_background_band(self.cmd, rect, top, bottom, band);
    }
    fn scene(&self, rect: [f32; 4], camera: &Camera, mode: RenderMode, band: Band) {
        self.scene.draw_scene(self.cmd, rect, camera, mode, band);
    }
}

/// OpenGL : tracés immédiats dans la cible courante, haute de `height` pixels.
pub struct GlDraw<'a> {
    pub dev: &'a GlDevice,
    pub scene: &'a GlScene,
    pub height: u32,
}

impl SceneDraw for GlDraw<'_> {
    fn set_params(&self, camera: &Camera, params: &SceneParams<'_>) {
        self.scene.set_params(self.dev, camera, params);
    }
    fn background(&self, rect: [f32; 4], top: [f32; 4], bottom: [f32; 4], band: Band) {
        self.scene.draw_background_band(self.dev, rect, self.height, top, bottom, band);
    }
    fn scene(&self, rect: [f32; 4], camera: &Camera, mode: RenderMode, band: Band) {
        self.scene.draw_scene(self.dev, rect, self.height, camera, mode, band);
    }
}

pub fn draw_viewport(scene: &dyn SceneDraw, state: &AppState, theme: &Theme, scale: f32) {
    draw_view(scene, state, theme, state.viewport.get(), scale, Target::Screen);
}

/// Destination d'une image de la vue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Écran : transitions en cours comprises.
    Screen,
    /// Capture : état au repos ; `transparent` : sans fond.
    Capture { transparent: bool },
}

/// Comme [`draw_viewport`] pour une vue `v` (pixels logiques) quelconque.
pub fn draw_view(scene: &dyn SceneDraw, state: &AppState, theme: &Theme, v: obv_ui::Rect, scale: f32, target: Target) {
    let animated = target == Target::Screen;
    let transparent = target == Target::Capture { transparent: true };
    let rect = [v.min.x * scale, v.min.y * scale, v.max.x * scale, v.max.y * scale];
    let c = &theme.color;
    let cam = &state.camera;
    let offsets = tools::explode_offsets(state);
    let params =
        SceneParams { section: tools::section_plane(state), offsets: &offsets, light: crate::lighting::block(state) };
    scene.set_params(cam, &params);
    // Vue normale et bandes des rendus selon les limites animées (au repos : l'état exact de l'inspecteur).
    let cuts = if animated { state.inspector_cuts } else { inspector::target_cuts(state.inspector) };
    let top = [c.viewport_top.r, c.viewport_top.g, c.viewport_top.b, 1.0];
    let bottom = [c.viewport_bottom.r, c.viewport_bottom.g, c.viewport_bottom.b, 1.0];
    let gray = inspector::background(theme.kind == ThemeKind::Dark);
    for (m, band) in inspector::device_regions(v, &cuts, scale) {
        // Alpha du haut : 1 autorise le fond d'environnement (vue normale), 0 le refuse (bandes des rendus).
        let (t, b) = if m.is_some() { ([gray[0], gray[1], gray[2], 0.0], gray) } else { (top, bottom) };
        if !transparent {
            scene.background(rect, t, b, band);
        }
        scene.scene(rect, cam, m.unwrap_or(RenderMode::Lit), band);
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
    if let Some(env) = &state.lighting.env {
        scene.set_environment(env)?;
    }
    let edges = (state.inspector != Inspector::Off).then(|| EdgeCache::compute(model));
    scene.set_meshes(&mesh_inputs(model, state, accent, edges.as_ref()))?;
    let v = state.viewport.get();
    let scale = w as f32 / v.width().max(1.0);
    let view = obv_ui::Rect::from_xywh(0.0, 0.0, v.width(), v.height());
    let clear = if transparent { [0.0; 4] } else { [0.0, 0.0, 0.0, 1.0] };
    target.render(clear, |cmd| {
        draw_view(&VkDraw { scene: &scene, cmd }, state, theme, view, scale, Target::Capture { transparent })
    })?;
    let rgba = target.read_rgba8();
    Ok(if ss == 1 { rgba } else { downsample(&rgba, w, h, ss) })
}

/// Comme [`capture`] sous OpenGL, avec la scène déjà chargée (arêtes comprises quand une vue de rendu est
/// ouverte).
#[allow(clippy::too_many_arguments)]
pub fn capture_gl(
    dev: &GlDevice,
    scene: &GlScene,
    state: &AppState,
    theme: &Theme,
    width: u32,
    height: u32,
    transparent: bool,
) -> std::result::Result<Vec<u8>, obv_gl::GlError> {
    let ss = if width.max(height) * 2 <= 8192 { 2 } else { 1 };
    let (w, h) = (width * ss, height * ss);
    let target = GlTarget::new(dev, w, h)?;
    let v = state.viewport.get();
    let scale = w as f32 / v.width().max(1.0);
    let view = obv_ui::Rect::from_xywh(0.0, 0.0, v.width(), v.height());
    let clear = if transparent { [0.0; 4] } else { [0.0, 0.0, 0.0, 1.0] };
    target.begin(dev, clear);
    draw_view(&GlDraw { dev, scene, height: h }, state, theme, view, scale, Target::Capture { transparent });
    let rgba = target.read_rgba8(dev);
    target.destroy(dev);
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

    /// Environnement d'éclairage (D-33).
    pub fn set_environment(&mut self, env: &obv_render::light::Environment) -> Result<()> {
        self.scene.set_environment(env)
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
                draw_viewport(&VkDraw { scene, cmd }, state, theme, scale);
                renderer.borrow().draw(cmd, extent);
            },
        )?;
        prepare_result?;
        Ok(self.target.read_rgba8())
    }
}

/// Image complète sous OpenGL, sans fenêtre (essais, D-30) : même contenu qu'[`OffscreenFrame`].
#[cfg(target_os = "linux")]
pub struct GlOffscreenFrame {
    pub headless: obv_gl::headless::Headless,
    target: Option<GlTarget>,
    scene: Option<GlScene>,
    ui: Option<obv_gl::GlUi>,
    edges: Option<EdgeCache>,
}

#[cfg(target_os = "linux")]
impl GlOffscreenFrame {
    pub fn new(width: u32, height: u32) -> std::result::Result<Self, obv_gl::GlError> {
        let headless = obv_gl::headless::Headless::new()?;
        let dev = &headless.device;
        let target = GlTarget::new(dev, width, height)?;
        let scene = GlScene::new(dev)?;
        let ui = obv_gl::GlUi::new(dev, 2048)?;
        Ok(GlOffscreenFrame { target: Some(target), scene: Some(scene), ui: Some(ui), headless, edges: None })
    }

    pub fn set_model(
        &mut self,
        model: &Model,
        state: &AppState,
        accent: [u8; 4],
    ) -> std::result::Result<(), obv_gl::GlError> {
        if state.inspector != Inspector::Off && self.edges.is_none() {
            self.edges = Some(EdgeCache::compute(model));
        }
        let meshes = mesh_inputs(model, state, accent, self.edges.as_ref());
        self.scene.as_mut().unwrap().set_meshes(&self.headless.device, &meshes)
    }

    /// Environnement d'éclairage (D-33).
    pub fn set_environment(
        &mut self,
        env: &obv_render::light::Environment,
    ) -> std::result::Result<(), obv_gl::GlError> {
        self.scene.as_mut().unwrap().set_environment(&self.headless.device, env)
    }

    pub fn render(&mut self, ui: &mut Ui, state: &AppState) -> Vec<u8> {
        let dev = &self.headless.device;
        let (target, scene, glui) =
            (self.target.as_ref().unwrap(), self.scene.as_ref().unwrap(), self.ui.as_mut().unwrap());
        let c = &ui.theme.color;
        target.begin(dev, [c.app_bg.r, c.app_bg.g, c.app_bg.b, 1.0]);
        draw_viewport(&GlDraw { dev, scene, height: target.height }, state, &ui.theme, ui.dl.scale);
        glui.prepare(dev, &ui.dl, &mut ui.atlas);
        glui.draw(dev, target.width, target.height);
        target.read_rgba8(dev)
    }
}

#[cfg(target_os = "linux")]
impl Drop for GlOffscreenFrame {
    fn drop(&mut self) {
        let dev = &self.headless.device;
        if let Some(t) = self.target.take() {
            t.destroy(dev);
        }
        if let Some(s) = self.scene.take() {
            s.destroy(dev);
        }
        if let Some(u) = self.ui.take() {
            u.destroy(dev);
        }
    }
}
