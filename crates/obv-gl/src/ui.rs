//! Interface sous OpenGL : comme `obv_ui::gpu::UiRenderer` (un tracé instancié, primitives en tampon de
//! stockage, atlas 8 bits).

use glow::HasContext;
use obv_ui::{
    Atlas, DrawList, Photos, gl_src,
    photos::{PHOTO_H, PHOTO_LAYERS, PHOTO_MIPS, PHOTO_W},
};

use crate::{GlDevice, GlError, Result, blend, depth, program, upload};

pub struct GlUi {
    program: glow::Program,
    prims: glow::Buffer,
    push: glow::Buffer,
    atlas: glow::Texture,
    atlas_size: u32,
    atlas_ready: bool,
    /// Photos (vignettes) : tableau de couches sRGB à niveaux de détail, unité de texture 1.
    photos: glow::Texture,
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
            let photos = gl.create_texture().map_err(GlError::Other)?;
            gl.bind_texture(glow::TEXTURE_2D_ARRAY, Some(photos));
            gl.tex_storage_3d(
                glow::TEXTURE_2D_ARRAY,
                PHOTO_MIPS as i32,
                glow::SRGB8_ALPHA8,
                PHOTO_W as i32,
                PHOTO_H as i32,
                PHOTO_LAYERS as i32,
            );
            for (k, v) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_MAX_LEVEL, PHOTO_MIPS - 1),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D_ARRAY, k, v as i32);
            }
            gl.bind_texture(glow::TEXTURE_2D_ARRAY, None);
            Ok(GlUi {
                photos,
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

    /// Envoie les primitives de l'image, la partie modifiée de l'atlas et les photos en attente.
    pub fn prepare(&mut self, dev: &GlDevice, dl: &DrawList, atlas: &mut Atlas, photos: &mut Photos) {
        let gl = &dev.gl;
        if !photos.pending.is_empty() {
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D_ARRAY, Some(self.photos));
                gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
                for (id, mips) in std::mem::take(&mut photos.pending) {
                    for (level, m) in mips.iter().enumerate() {
                        gl.tex_sub_image_3d(
                            glow::TEXTURE_2D_ARRAY,
                            level as i32,
                            0,
                            0,
                            id.0 as i32,
                            (PHOTO_W >> level).max(1) as i32,
                            (PHOTO_H >> level).max(1) as i32,
                            1,
                            glow::RGBA,
                            glow::UNSIGNED_BYTE,
                            glow::PixelUnpackData::Slice(Some(m)),
                        );
                    }
                }
                gl.bind_texture(glow::TEXTURE_2D_ARRAY, None);
            }
        }
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
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D_ARRAY, Some(self.photos));
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays_instanced(glow::TRIANGLES, 0, 6, self.count);
            gl.bind_texture(glow::TEXTURE_2D_ARRAY, None);
            gl.active_texture(glow::TEXTURE0);
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
            gl.delete_texture(self.photos);
            gl.delete_vertex_array(self.vao);
        }
    }
}
