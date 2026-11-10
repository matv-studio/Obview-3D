//! Interface sous OpenGL : comme `obv_ui::gpu::UiRenderer` (un tracé instancié, primitives en tampon de
//! stockage, atlas 8 bits).

use glow::HasContext;
use obv_ui::{Atlas, DrawList, gl_src};

use crate::{GlDevice, GlError, Result, blend, depth, program, upload};

pub struct GlUi {
    program: glow::Program,
    prims: glow::Buffer,
    push: glow::Buffer,
    atlas: glow::Texture,
    atlas_size: u32,
    atlas_ready: bool,
    count: i32,
    vao: glow::VertexArray,
}

impl GlUi {
    pub fn new(dev: &GlDevice, atlas_size: u32) -> Result<Self> {
        let gl = &dev.gl;
        let program = program(gl, gl_src::UI_VERT, gl_src::UI_FRAG, false)?;
        unsafe {
            let atlas = gl.create_texture().map_err(GlError::Other)?;
            gl.bind_texture(glow::TEXTURE_2D, Some(atlas));
            gl.tex_storage_2d(glow::TEXTURE_2D, 1, glow::R8, atlas_size as i32, atlas_size as i32);
            for (k, v) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, k, v as i32);
            }
            gl.bind_texture(glow::TEXTURE_2D, None);
            Ok(GlUi {
                program,
                prims: gl.create_buffer().map_err(GlError::Other)?,
                push: gl.create_buffer().map_err(GlError::Other)?,
                atlas,
                atlas_size,
                atlas_ready: false,
                count: 0,
                vao: gl.create_vertex_array().map_err(GlError::Other)?,
            })
        }
    }

    /// Envoie les primitives de l'image et la partie modifiée de l'atlas.
    pub fn prepare(&mut self, dev: &GlDevice, dl: &DrawList, atlas: &mut Atlas) {
        let gl = &dev.gl;
        let dirty = atlas.dirty.take();
        let region = if self.atlas_ready { dirty } else { Some([0, 0, atlas.size, atlas.size]) };
        if let Some([x0, y0, x1, y1]) = region
            && atlas.size == self.atlas_size
        {
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D, Some(self.atlas));
                gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
                gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, atlas.size as i32);
                gl.pixel_store_i32(glow::UNPACK_SKIP_PIXELS, x0 as i32);
                gl.pixel_store_i32(glow::UNPACK_SKIP_ROWS, y0 as i32);
                gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    x0 as i32,
                    y0 as i32,
                    (x1 - x0) as i32,
                    (y1 - y0) as i32,
                    glow::RED,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(&atlas.pixels)),
                );
                gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
                gl.pixel_store_i32(glow::UNPACK_SKIP_PIXELS, 0);
                gl.pixel_store_i32(glow::UNPACK_SKIP_ROWS, 0);
                gl.bind_texture(glow::TEXTURE_2D, None);
            }
            self.atlas_ready = true;
        }
        let bytes: &[u8] = bytemuck::cast_slice(&dl.prims);
        upload(gl, self.prims, glow::SHADER_STORAGE_BUFFER, bytes, glow::STREAM_DRAW);
        self.count = dl.prims.len() as i32;
    }

    /// Trace l'interface sur toute la cible (`width` × `height` pixels).
    pub fn draw(&self, dev: &GlDevice, width: u32, height: u32) {
        if self.count == 0 || !self.atlas_ready {
            return;
        }
        let gl = &dev.gl;
        let screen: [f32; 4] = [width as f32, height as f32, 0.0, 0.0];
        upload(gl, self.push, glow::UNIFORM_BUFFER, bytemuck::cast_slice(&screen), glow::STREAM_DRAW);
        depth(gl, false, false, true);
        blend(gl, true);
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.viewport(0, 0, width as i32, height as i32);
            gl.bind_buffer_base(glow::UNIFORM_BUFFER, 0, Some(self.push));
            gl.bind_buffer_base(glow::SHADER_STORAGE_BUFFER, 0, Some(self.prims));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.atlas));
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays_instanced(glow::TRIANGLES, 0, 6, self.count);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    pub fn destroy(self, dev: &GlDevice) {
        let gl = &dev.gl;
        unsafe {
            gl.delete_program(self.program);
            gl.delete_buffer(self.prims);
            gl.delete_buffer(self.push);
            gl.delete_texture(self.atlas);
            gl.delete_vertex_array(self.vao);
        }
    }
}
