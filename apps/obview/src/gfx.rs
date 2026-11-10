//! Moteur de rendu de la fenêtre (D-30) : Vulkan 1.3 d'abord, OpenGL 4.3 en secours. Le choix se fait au
//! démarrage (réglage Affichage › Moteur, `--gl` ou `OBV_RENDERER=gl|vulkan`) et peut changer sans redémarrer
//! (la fenêtre est recréée à la même place).

use std::{num::NonZeroU32, sync::Arc};

use glutin::{
    config::ConfigTemplateBuilder,
    context::{ContextApi, ContextAttributesBuilder, GlProfile, PossiblyCurrentContext, Version},
    display::GetGlDisplay,
    prelude::*,
    surface::{Surface, SwapInterval, WindowSurface},
};
use glutin_winit::{DisplayBuilder, GlWindow};
use obv_gl::{GlDevice, GlScene, GlTarget, GlUi};
use obv_gpu::{FRAMES_IN_FLIGHT, Gpu, GpuDesc, PresentMode, Swapchain};
use obv_io::Model;
use obv_render::{Camera, MeshInput, Pick, SceneRenderer};
use obv_ui::{Ui, gpu::UiRenderer};
use winit::{
    event_loop::ActiveEventLoop,
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::{Window, WindowAttributes},
};

use crate::{
    frame::{self, GlDraw, VkDraw, draw_viewport},
    settings::RendererChoice,
    shell::AppState,
};

/// Moteur actif.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Vulkan,
    OpenGl,
}

struct VkGfx {
    // Ordre de libération : la chaîne d'échange et les tracés avant l'appareil.
    swapchain: Swapchain,
    scene: SceneRenderer,
    ui: UiRenderer,
    gpu: Gpu,
}

struct GlGfx {
    // Ressources OpenGL détruites explicitement (contexte courant), avant la surface et le contexte.
    scene: Option<GlScene>,
    ui: Option<GlUi>,
    target: Option<GlTarget>,
    dev: GlDevice,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
}

impl Drop for GlGfx {
    fn drop(&mut self) {
        if let Some(s) = self.scene.take() {
            s.destroy(&self.dev);
        }
        if let Some(u) = self.ui.take() {
            u.destroy(&self.dev);
        }
        if let Some(t) = self.target.take() {
            t.destroy(&self.dev);
        }
    }
}

enum Inner {
    Vulkan(Box<VkGfx>),
    Gl(Box<GlGfx>),
}

/// Fenêtre et moteur. Ordre des champs = ordre de libération : le moteur avant la fenêtre.
pub struct Gfx {
    inner: Inner,
    pub window: Arc<Window>,
    pub backend: Backend,
    /// « Vulkan 1.4 · NVIDIA GeForce … », « OpenGL 4.6 · … » (réglages).
    pub description: String,
}

impl Gfx {
    /// Crée la fenêtre et le moteur demandé ; l'autre moteur prend le relais si le premier échoue. Le second
    /// résultat est le message de l'échec du moteur demandé, le cas échéant.
    pub fn create(
        el: &ActiveEventLoop,
        attrs: WindowAttributes,
        choice: RendererChoice,
        validation: bool,
    ) -> Result<(Gfx, Option<String>), String> {
        let order = match choice {
            RendererChoice::OpenGl => [Backend::OpenGl, Backend::Vulkan],
            _ => [Backend::Vulkan, Backend::OpenGl],
        };
        let mut first_error = None;
        for b in order {
            let r = match b {
                Backend::Vulkan => Self::vulkan(el, attrs.clone(), validation),
                Backend::OpenGl => Self::opengl(el, attrs.clone()),
            };
            match r {
                Ok(g) => {
                    // Un échec du moteur demandé n'est signalé que s'il a été choisi explicitement.
                    let report = if choice == RendererChoice::Auto { None } else { first_error };
                    return Ok((g, report));
                }
                Err(e) => {
                    eprintln!("Obview 3D : {} indisponible : {e}", b.name());
                    first_error.get_or_insert(e);
                }
            }
        }
        Err(first_error.unwrap_or_else(|| "aucun moteur de rendu".into()))
    }

    fn vulkan(el: &ActiveEventLoop, attrs: WindowAttributes, validation: bool) -> Result<Gfx, String> {
        // Appareil d'abord : sans Vulkan, aucune fenêtre n'est créée pour rien.
        let display = el.display_handle().map_err(|e| e.to_string())?.as_raw();
        let exts = obv_gpu::required_instance_extensions(display).map_err(|e| e.to_string())?;
        let gpu = Gpu::new(&GpuDesc { validation, instance_extensions: exts, presentation: true })
            .map_err(|e| e.to_string())?;
        let window = Arc::new(el.create_window(attrs).map_err(|e| e.to_string())?);
        let handle = window.window_handle().map_err(|e| e.to_string())?.as_raw();
        let size = window.inner_size();
        // SAFETY: la fenêtre vit dans `Gfx` aussi longtemps que la chaîne d'échange (ordre des champs).
        let swapchain = unsafe {
            Swapchain::new(&gpu, display, handle, (size.width, size.height), PresentMode::LowLatency)
                .map_err(|e| e.to_string())?
        };
        let scene = SceneRenderer::new(&gpu, swapchain.format, swapchain.depth_format).map_err(|e| e.to_string())?;
        let ui = UiRenderer::new(&gpu, swapchain.format, Some(swapchain.depth_format), 2048, FRAMES_IN_FLIGHT)
            .map_err(|e| e.to_string())?;
        let c = gpu.caps();
        let description = format!(
            "Vulkan {}.{} · {}",
            obv_gpu::vk::api_version_major(c.api_version),
            obv_gpu::vk::api_version_minor(c.api_version),
            c.device_name
        );
        Ok(Gfx {
            inner: Inner::Vulkan(Box::new(VkGfx { swapchain, scene, ui, gpu })),
            window,
            backend: Backend::Vulkan,
            description,
        })
    }

    fn opengl(el: &ActiveEventLoop, attrs: WindowAttributes) -> Result<Gfx, String> {
        // Couleur 8 bits sans profondeur ni multiéchantillonnage : tout est tracé hors écran puis recopié.
        let template = ConfigTemplateBuilder::new().with_alpha_size(8).with_depth_size(0);
        let (window, config) = DisplayBuilder::new()
            .with_window_attributes(Some(attrs))
            .build(el, template, |configs| {
                configs.min_by_key(|c| (c.num_samples(), c.depth_size())).expect("aucune configuration OpenGL")
            })
            .map_err(|e| e.to_string())?;
        let window = Arc::new(window.ok_or("fenêtre OpenGL non créée")?);
        let display = config.display();
        let raw = window.window_handle().map_err(|e| e.to_string())?.as_raw();
        let mut not_current = None;
        for (major, minor) in [(4, 6), (4, 5), (4, 3)] {
            let a = ContextAttributesBuilder::new()
                .with_context_api(ContextApi::OpenGl(Some(Version::new(major, minor))))
                .with_profile(GlProfile::Core)
                .build(Some(raw));
            // SAFETY: configuration et fenêtre valides, la fenêtre survit au contexte (ordre des champs).
            if let Ok(c) = unsafe { display.create_context(&config, &a) } {
                not_current = Some(c);
                break;
            }
        }
        let not_current = not_current.ok_or("aucun contexte OpenGL 4.3 cœur")?;
        let surface_attrs = window.build_surface_attributes(Default::default()).map_err(|e| e.to_string())?;
        // SAFETY: surface de la fenêtre créée ci-dessus.
        let surface = unsafe { display.create_window_surface(&config, &surface_attrs) }.map_err(|e| e.to_string())?;
        let context = not_current.make_current(&surface).map_err(|e| e.to_string())?;
        // Synchronisation verticale : pas de déchirure ; l'image n'est rendue qu'à la demande.
        let _ = surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::MIN));
        // SAFETY: contexte courant sur ce fil (celui de la boucle d'événements) pour toute la vie de `Gfx`.
        let dev = unsafe { GlDevice::new(|s| display.get_proc_address(s)) }.map_err(|e| e.to_string())?;
        let size = window.inner_size();
        let target = GlTarget::new(&dev, size.width, size.height).map_err(|e| e.to_string())?;
        let scene = GlScene::new(&dev).map_err(|e| e.to_string())?;
        let ui = GlUi::new(&dev, 2048).map_err(|e| e.to_string())?;
        let description = dev.description.clone();
        Ok(Gfx {
            inner: Inner::Gl(Box::new(GlGfx {
                scene: Some(scene),
                ui: Some(ui),
                target: Some(target),
                dev,
                surface,
                context,
            })),
            window,
            backend: Backend::OpenGl,
            description,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        match &mut self.inner {
            Inner::Vulkan(v) => v.swapchain.resize(width, height),
            Inner::Gl(g) => {
                if let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
                    g.surface.resize(&g.context, w, h);
                    if let Some(t) = &mut g.target
                        && let Err(e) = t.resize(&g.dev, width, height)
                    {
                        eprintln!("Obview 3D : {e}");
                    }
                }
            }
        }
    }

    pub fn set_meshes(&mut self, meshes: &[MeshInput<'_>]) -> Result<(), String> {
        match &mut self.inner {
            Inner::Vulkan(v) => v.scene.set_meshes(meshes).map_err(|e| e.to_string()),
            Inner::Gl(g) => match &mut g.scene {
                Some(s) => s.set_meshes(&g.dev, meshes).map_err(|e| e.to_string()),
                None => Ok(()),
            },
        }
    }

    /// Sélection au pixel `px` d'une vue `size` (pixels de la cible).
    pub fn pick(&self, camera: &Camera, size: [f32; 2], px: [f32; 2]) -> Option<Pick> {
        match &self.inner {
            Inner::Vulkan(v) => v.scene.pick(camera, size, px).ok().flatten(),
            Inner::Gl(g) => g.scene.as_ref()?.pick(&g.dev, camera, size, px),
        }
    }

    /// Rend et présente une image : vue 3D puis interface.
    pub fn present(&mut self, ui: &mut Ui, state: &AppState, scale: f32) -> Result<(), String> {
        let c = &ui.theme.color;
        let bg = [c.app_bg.r, c.app_bg.g, c.app_bg.b, 1.0];
        match &mut self.inner {
            Inner::Vulkan(v) => {
                let extent = v.swapchain.extent;
                let (ui_r, scene) = (&mut v.ui, &v.scene);
                let (dl, atlas, theme) = (&ui.dl, &mut ui.atlas, &ui.theme);
                let mut prep_err = None;
                let ui_cell = std::cell::RefCell::new(ui_r);
                let r = v.swapchain.present(
                    bg,
                    |cmd, slot| {
                        if let Err(e) = ui_cell.borrow_mut().prepare(cmd, slot, dl, atlas) {
                            prep_err = Some(e);
                        }
                    },
                    |cmd| {
                        draw_viewport(&VkDraw { scene, cmd }, state, theme, scale);
                        ui_cell.borrow().draw(cmd, extent);
                    },
                );
                r.map_err(|e| e.to_string())?;
                prep_err.map_or(Ok(()), |e| Err(e.to_string()))
            }
            Inner::Gl(g) => {
                let (Some(target), Some(scene), Some(glui)) = (&mut g.target, &g.scene, &mut g.ui) else {
                    return Ok(());
                };
                target.begin(&g.dev, bg);
                draw_viewport(&GlDraw { dev: &g.dev, scene, height: target.height }, state, &ui.theme, scale);
                glui.prepare(&g.dev, &ui.dl, &mut ui.atlas);
                glui.draw(&g.dev, target.width, target.height);
                target.present(&g.dev).map_err(|e| e.to_string())?;
                g.surface.swap_buffers(&g.context).map_err(|e| e.to_string())
            }
        }
    }

    /// Image de la vue seule, `width` × `height`, sans l'interface.
    #[allow(clippy::too_many_arguments)]
    pub fn capture(
        &self,
        model: &Model,
        state: &AppState,
        theme: &obv_ui::Theme,
        accent: [u8; 4],
        width: u32,
        height: u32,
        transparent: bool,
    ) -> Result<Vec<u8>, String> {
        match &self.inner {
            Inner::Vulkan(v) => frame::capture(&v.gpu, model, state, theme, accent, width, height, transparent)
                .map_err(|e| e.to_string()),
            Inner::Gl(g) => {
                let scene = g.scene.as_ref().ok_or("scène absente")?;
                frame::capture_gl(&g.dev, scene, state, theme, width, height, transparent).map_err(|e| e.to_string())
            }
        }
    }

    pub fn validation_errors(&self) -> usize {
        match &self.inner {
            Inner::Vulkan(v) => v.gpu.validation_errors(),
            Inner::Gl(_) => 0,
        }
    }
}

impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Backend::Vulkan => "Vulkan",
            Backend::OpenGl => "OpenGL",
        }
    }
}
