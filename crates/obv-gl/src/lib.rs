//! Moteur OpenGL de secours d'Obview (D-30), choisi quand Vulkan 1.3 manque (pilote absent, machine virtuelle,
//! carte ancienne).
//!
//! - OpenGL 4.30 cœur au minimum (tampons de stockage lus par les nuanceurs de sommets) ; `glClipControl`
//!   (4.5 ou `ARB_clip_control`) quand il existe, sinon la profondeur est ramenée dans les nuanceurs : la
//!   profondeur inversée de Vulkan (0 à 1) est gardée dans les deux cas.
//! - Mêmes nuanceurs que Vulkan : sources GLSL communes, prétraitées avec `OBV_GL` à la construction
//!   (`obv_render::gl_src`, `obv_ui::gl_src`).
//! - Tout est tracé dans une cible hors écran (couleur sRGB, profondeur flottante 32 bits), puis recopiée dans la
//!   fenêtre : même précision, même encodage des couleurs et même lecture qu'avec Vulkan.

use std::ffi::{CStr, c_void};

use glow::HasContext;

mod scene;
mod target;
mod ui;

#[cfg(target_os = "linux")]
pub mod headless;

pub use glow;
pub use scene::GlScene;
pub use target::GlTarget;
pub use ui::GlUi;

#[derive(Debug, thiserror::Error)]
pub enum GlError {
    #[error("OpenGL {0} : 4.3 au moins est nécessaire")]
    Version(String),
    #[error("OpenGL : {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, GlError>;

type ClipControl = unsafe extern "system" fn(u32, u32);

/// Contexte OpenGL courant et ce qu'il permet.
pub struct GlDevice {
    pub gl: glow::Context,
    /// « OpenGL 4.6 · Mesa llvmpipe … » (réglages, journal).
    pub description: String,
    /// `glClipControl` présent : profondeur de 0 à 1 sans retouche des nuanceurs.
    pub clip_control: bool,
}

impl GlDevice {
    /// Charge les fonctions par `loader` (contexte déjà courant) et vérifie la version.
    ///
    /// # Safety
    /// Un contexte OpenGL doit être courant sur ce fil, et le rester tant que l'appareil sert.
    pub unsafe fn new(loader: impl Fn(&CStr) -> *const c_void) -> Result<Self> {
        let gl = unsafe { glow::Context::from_loader_function_cstr(&loader) };
        let v = gl.version();
        let (vendor, renderer, version) = unsafe {
            (
                gl.get_parameter_string(glow::VENDOR),
                gl.get_parameter_string(glow::RENDERER),
                gl.get_parameter_string(glow::VERSION),
            )
        };
        let description = format!("OpenGL {}.{} · {renderer} ({vendor})", v.major, v.minor);
        if v.is_embedded || (v.major, v.minor) < (4, 3) {
            return Err(GlError::Version(version));
        }
        // Les positions sont lues par le nuanceur de sommets : au moins cinq tampons de stockage à cet étage.
        let vertex_ssbo = unsafe { gl.get_parameter_i32(glow::MAX_VERTEX_SHADER_STORAGE_BLOCKS) };
        if vertex_ssbo < 5 {
            return Err(GlError::Other(format!("{vertex_ssbo} tampons de stockage au nuanceur de sommets (5 requis)")));
        }
        let clip = loader(c"glClipControl");
        // `OBV_GL_NO_CLIP_CONTROL` : chemin des pilotes sans glClipControl (essais).
        let clip_control = !clip.is_null()
            && std::env::var_os("OBV_GL_NO_CLIP_CONTROL").is_none()
            && ((v.major, v.minor) >= (4, 5) || gl.supported_extensions().contains("GL_ARB_clip_control"));
        if clip_control {
            // SAFETY: pointeur fourni par le chargeur pour cette fonction, signature de la spécification.
            let f: ClipControl = unsafe { std::mem::transmute::<*const c_void, ClipControl>(clip) };
            unsafe { f(glow::LOWER_LEFT, glow::ZERO_TO_ONE) };
        }
        Ok(GlDevice { gl, description, clip_control })
    }
}

/// Programme lié depuis deux sources ; `remap` : profondeur ramenée dans le nuanceur (sans `glClipControl`).
pub(crate) fn program(gl: &glow::Context, vert: &str, frag: &str, remap: bool) -> Result<glow::Program> {
    // Après la ligne `#version` : profondeur ramenée ou non dans les nuanceurs de sommets.
    let inject = |src: &str| -> String {
        let def = format!("#define OBV_DEPTH_REMAP {}\n", if remap { "1.0" } else { "0.0" });
        match src.split_once('\n') {
            Some((first, rest)) => format!("{first}\n{def}{rest}"),
            None => src.to_owned(),
        }
    };
    unsafe {
        let p = gl.create_program().map_err(GlError::Other)?;
        let mut shaders = Vec::new();
        for (kind, src) in [(glow::VERTEX_SHADER, inject(vert)), (glow::FRAGMENT_SHADER, inject(frag))] {
            let s = gl.create_shader(kind).map_err(GlError::Other)?;
            gl.shader_source(s, &src);
            gl.compile_shader(s);
            if !gl.get_shader_compile_status(s) {
                return Err(GlError::Other(format!("compilation : {}", gl.get_shader_info_log(s))));
            }
            gl.attach_shader(p, s);
            shaders.push(s);
        }
        gl.link_program(p);
        for s in shaders {
            gl.detach_shader(p, s);
            gl.delete_shader(s);
        }
        if !gl.get_program_link_status(p) {
            return Err(GlError::Other(format!("édition des liens : {}", gl.get_program_info_log(p))));
        }
        Ok(p)
    }
}

/// Tampon de stockage ou uniforme rempli de `bytes` (au moins 16 octets : un tampon vide ne se lie pas).
pub(crate) fn upload(gl: &glow::Context, buffer: glow::Buffer, target: u32, bytes: &[u8], usage: u32) {
    unsafe {
        gl.bind_buffer(target, Some(buffer));
        if bytes.len() < 16 {
            let mut b = bytes.to_vec();
            b.resize(16, 0);
            gl.buffer_data_u8_slice(target, &b, usage);
        } else {
            gl.buffer_data_u8_slice(target, bytes, usage);
        }
        gl.bind_buffer(target, None);
    }
}

/// Fenêtre d'affichage et découpe sur le rectangle `rect` (pixels, origine en haut à gauche, comme Vulkan) d'une
/// cible de hauteur `height` (OpenGL compte depuis le bas). `None` : rectangle vide.
pub(crate) fn set_rect(gl: &glow::Context, rect: [f32; 4], height: u32) -> Option<(f32, f32)> {
    let (x, y, w, h) = (rect[0], rect[1], rect[2] - rect[0], rect[3] - rect[1]);
    if w < 1.0 || h < 1.0 {
        return None;
    }
    let gy = height as f32 - (y + h);
    unsafe {
        gl.viewport(x as i32, gy as i32, w as i32, h as i32);
        gl.enable(glow::SCISSOR_TEST);
        gl.scissor(x as i32, gy as i32, w as i32, h as i32);
    }
    Some((w, h))
}

/// Mélange prémultiplié (comme les pipelines Vulkan avec `alpha_blend`).
pub(crate) fn blend(gl: &glow::Context, on: bool) {
    unsafe {
        if on {
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            gl.blend_func_separate(glow::ONE, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
        } else {
            gl.disable(glow::BLEND);
        }
    }
}

/// Test de profondeur : `write` écrit la profondeur ; `always` passe toujours (fond, rayons X, interface).
pub(crate) fn depth(gl: &glow::Context, test: bool, write: bool, always: bool) {
    unsafe {
        if test {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(if always { glow::ALWAYS } else { glow::GEQUAL });
        } else {
            gl.disable(glow::DEPTH_TEST);
        }
        gl.depth_mask(write);
    }
}
