//! Contexte OpenGL sans fenêtre (EGL sur un appareil, sans surface) : essais et comparaisons avec Vulkan sous
//! Linux (Mesa llvmpipe en CI).

use glutin::{
    api::egl::{context::PossiblyCurrentContext, device::Device, display::Display},
    config::{ConfigSurfaceTypes, ConfigTemplateBuilder},
    context::{ContextApi, ContextAttributesBuilder, GlProfile, Version},
    prelude::*,
};

use crate::{GlDevice, GlError, Result};

/// Contexte courant sur ce fil, avec son appareil OpenGL.
pub struct Headless {
    pub device: GlDevice,
    _context: PossiblyCurrentContext,
    _display: Display,
}

impl Headless {
    /// Premier appareil EGL qui fournit un contexte OpenGL 4.3 cœur (`OBV_GL_DEVICE` : sous-chaîne du nom du
    /// pilote à préférer, ex. « llvmpipe »).
    pub fn new() -> Result<Self> {
        let err = |e: glutin::error::Error| GlError::Other(e.to_string());
        let want = std::env::var("OBV_GL_DEVICE").ok();
        let mut devices: Vec<Device> = Device::query_devices().map_err(err)?.collect();
        if let Some(w) = &want {
            devices.sort_by_key(|d| !d.extensions().iter().any(|e| e.contains(w.as_str())));
        }
        let mut last = GlError::Other("aucun appareil EGL".into());
        for dev in devices {
            // SAFETY: aucun descripteur DRM fourni.
            let display = match unsafe { Display::with_device(&dev, None) } {
                Ok(d) => d,
                Err(e) => {
                    last = err(e);
                    continue;
                }
            };
            let template = ConfigTemplateBuilder::new()
                .with_surface_type(ConfigSurfaceTypes::empty())
                .with_api(glutin::config::Api::OPENGL)
                .build();
            // SAFETY: configuration tirée de cet affichage.
            let Some(config) = (unsafe { display.find_configs(template) }).ok().and_then(|mut c| c.next()) else {
                continue;
            };
            let attrs = ContextAttributesBuilder::new()
                .with_context_api(ContextApi::OpenGl(Some(Version::new(4, 5))))
                .with_profile(GlProfile::Core)
                .build(None);
            let fallback = ContextAttributesBuilder::new()
                .with_context_api(ContextApi::OpenGl(Some(Version::new(4, 3))))
                .with_profile(GlProfile::Core)
                .build(None);
            // SAFETY: configuration et affichage valides.
            let ctx = unsafe { display.create_context(&config, &attrs) }
                .or_else(|_| unsafe { display.create_context(&config, &fallback) });
            let ctx = match ctx {
                Ok(c) => c,
                Err(e) => {
                    last = err(e);
                    continue;
                }
            };
            let ctx = match ctx.make_current_surfaceless() {
                Ok(c) => c,
                Err(e) => {
                    last = err(e);
                    continue;
                }
            };
            // SAFETY: le contexte vient d'être rendu courant sur ce fil.
            match unsafe { GlDevice::new(|s| display.get_proc_address(s)) } {
                Ok(device) => return Ok(Headless { device, _context: ctx, _display: display }),
                Err(e) => last = e,
            }
        }
        Err(last)
    }
}
