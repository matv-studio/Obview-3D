//! Cible hors écran : couleur sRGB 8 bits, profondeur flottante 32 bits (comme `obv_gpu::RenderTarget`), et
//! recopie dans la fenêtre.

use glow::HasContext;

use crate::{GlDevice, GlError, Result};

const COPY_VERT: &str = "#version 430 core
void main() {
    vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
";

// La lecture d'une texture sRGB rend des valeurs linéaires : réencodées ici, la fenêtre recevant les octets tels
// quels (FRAMEBUFFER_SRGB coupé), quel que soit le pilote.
const COPY_FRAG: &str = "#version 430 core
layout(binding = 0) uniform sampler2D src;
out vec4 o;
vec3 encode(vec3 c) {
    return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(0.0031308, c));
}
void main() {
    vec4 c = texelFetch(src, ivec2(gl_FragCoord.xy), 0);
    o = vec4(encode(c.rgb), 1.0);
}
";

pub struct GlTarget {
    pub width: u32,
    pub height: u32,
    fbo: glow::Framebuffer,
    color: glow::Texture,
    depth: glow::Renderbuffer,
    copy: Option<glow::Program>,
    vao: glow::VertexArray,
}

impl GlTarget {
    pub fn new(dev: &GlDevice, width: u32, height: u32) -> Result<Self> {
        let gl = &dev.gl;
        let (w, h) = (width.max(1), height.max(1));
        unsafe {
            let fbo = gl.create_framebuffer().map_err(GlError::Other)?;
            let color = gl.create_texture().map_err(GlError::Other)?;
            let depth = gl.create_renderbuffer().map_err(GlError::Other)?;
            let vao = gl.create_vertex_array().map_err(GlError::Other)?;
            let mut t = GlTarget { width: w, height: h, fbo, color, depth, copy: None, vao };
            t.allocate(gl)?;
            Ok(t)
        }
    }

    fn allocate(&mut self, gl: &glow::Context) -> Result<()> {
        let (w, h) = (self.width as i32, self.height as i32);
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.color));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::SRGB8_ALPHA8 as i32,
                w,
                h,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAX_LEVEL, 0);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(self.depth));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT32F, w, h);
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(self.color),
                0,
            );
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::RENDERBUFFER,
                Some(self.depth),
            );
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            if status != glow::FRAMEBUFFER_COMPLETE {
                return Err(GlError::Other(format!("cible hors écran incomplète ({status:#x})")));
            }
        }
        Ok(())
    }

    /// Nouvelle taille (contenu perdu).
    pub fn resize(&mut self, dev: &GlDevice, width: u32, height: u32) -> Result<()> {
        let (w, h) = (width.max(1), height.max(1));
        if (w, h) == (self.width, self.height) {
            return Ok(());
        }
        self.width = w;
        self.height = h;
        self.allocate(&dev.gl)
    }

    /// Cible courante, effacée : couleur `clear` (linéaire), profondeur 0 (profondeur inversée).
    pub fn begin(&self, dev: &GlDevice, clear: [f32; 4]) {
        let gl = &dev.gl;
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.enable(glow::FRAMEBUFFER_SRGB);
            gl.disable(glow::SCISSOR_TEST);
            gl.viewport(0, 0, self.width as i32, self.height as i32);
            gl.color_mask(true, true, true, true);
            gl.depth_mask(true);
            gl.clear_buffer_f32_slice(glow::COLOR, 0, &clear);
            gl.clear_buffer_f32_slice(glow::DEPTH, 0, &[0.0]);
            gl.bind_vertex_array(Some(self.vao));
        }
    }

    /// Image en RVBA 8 bits sRGB, lignes du haut vers le bas.
    pub fn read_rgba8(&self, dev: &GlDevice) -> Vec<u8> {
        let gl = &dev.gl;
        let (w, h) = (self.width as usize, self.height as usize);
        let mut px = vec![0u8; w * h * 4];
        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.fbo));
            gl.disable(glow::FRAMEBUFFER_SRGB);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut px)),
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
        }
        // OpenGL range les lignes du bas vers le haut.
        let row = w * 4;
        let mut out = vec![0u8; px.len()];
        for y in 0..h {
            out[y * row..(y + 1) * row].copy_from_slice(&px[(h - 1 - y) * row..(h - y) * row]);
        }
        out
    }

    /// Recopie dans le tampon de la fenêtre (0), de même taille.
    pub fn present(&mut self, dev: &GlDevice) -> Result<()> {
        let gl = &dev.gl;
        if self.copy.is_none() {
            self.copy = Some(crate::program(gl, COPY_VERT, COPY_FRAG, false)?);
        }
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.disable(glow::FRAMEBUFFER_SRGB);
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::BLEND);
            gl.viewport(0, 0, self.width as i32, self.height as i32);
            gl.use_program(self.copy);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.color));
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
        Ok(())
    }

    pub fn destroy(self, dev: &GlDevice) {
        let gl = &dev.gl;
        unsafe {
            gl.delete_framebuffer(self.fbo);
            gl.delete_texture(self.color);
            gl.delete_renderbuffer(self.depth);
            gl.delete_vertex_array(self.vao);
            if let Some(p) = self.copy {
                gl.delete_program(p);
            }
        }
    }
}
