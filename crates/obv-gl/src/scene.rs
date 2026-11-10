//! Scène 3D sous OpenGL : mêmes tracés que `obv_render::SceneRenderer` (fond, surfaces, coupe, rayons X, arêtes,
//! sélection au pixel), géométrie en tampons de stockage liés.

use std::cell::RefCell;

use glow::HasContext;
use obv_render::{
    Band, Camera, GeometryArrays, MeshInput, Pick, PickRead, RenderMode, SceneParams, encode_params, gl_src,
    resolve_pick,
};

use crate::{GlDevice, GlError, Result, blend, depth, program, set_rect, upload};

/// Bloc uniforme des surfaces (std140, 96 octets).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ScenePush {
    view_proj: [f32; 16],
    mode: u32,
    pad: [u32; 3],
    band: [f32; 4],
}

/// Bloc uniforme des arêtes (std140, 96 octets).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EdgePush {
    view_proj: [f32; 16],
    viewport: [f32; 2],
    width: f32,
    color: u32,
    band: [f32; 4],
}

/// Bloc uniforme du fond (std140, 112 octets).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BackgroundPush {
    top: [f32; 4],
    bottom: [f32; 4],
    band: [f32; 4],
    inv_view_proj: [f32; 16],
}

struct Geometry {
    positions: glow::Buffer,
    normals: glow::Buffer,
    indices: glow::Buffer,
    attrs: glow::Buffer,
    index_count: i32,
    edges: Option<(glow::Buffer, glow::Buffer)>,
    edge_count: i32,
    ranges: Vec<(u32, u32)>,
}

struct Programs {
    scene: glow::Program,
    section: glow::Program,
    xray: glow::Program,
    edges: glow::Program,
    pick: glow::Program,
    background: glow::Program,
}

struct PickTarget {
    fbo: glow::Framebuffer,
    ids: glow::Texture,
    depth: glow::Texture,
}

pub struct GlScene {
    programs: Programs,
    geometry: Option<Geometry>,
    params: glow::Buffer,
    push: glow::Buffer,
    pick_target: PickTarget,
    vao: glow::VertexArray,
    /// Coupe de l'image en cours (pour la sélection d'un bouchon).
    section: RefCell<Option<[f32; 4]>>,
    /// Caméra de l'image en cours (directions du fond).
    camera: RefCell<Camera>,
    /// Pyramide d'environnement (D-33), unité de texture 1.
    env: glow::Texture,
}

/// Texture RGBA16F à niveaux de l'environnement : longitude bouclée, latitude bornée.
unsafe fn env_texture(gl: &glow::Context, levels: &[(u32, u32, Vec<u16>)]) -> Result<glow::Texture> {
    unsafe {
        let t = gl.create_texture().map_err(GlError::Other)?;
        gl.bind_texture(glow::TEXTURE_2D, Some(t));
        let (w, h) = (levels[0].0 as i32, levels[0].1 as i32);
        gl.tex_storage_2d(glow::TEXTURE_2D, levels.len() as i32, glow::RGBA16F, w, h);
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        for (k, (lw, lh, data)) in levels.iter().enumerate() {
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                k as i32,
                0,
                0,
                *lw as i32,
                *lh as i32,
                glow::RGBA,
                glow::HALF_FLOAT,
                glow::PixelUnpackData::Slice(Some(bytemuck::cast_slice(data))),
            );
        }
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAX_LEVEL, levels.len() as i32 - 1);
        gl.bind_texture(glow::TEXTURE_2D, None);
        Ok(t)
    }
}

impl GlScene {
    pub fn new(dev: &GlDevice) -> Result<Self> {
        let gl = &dev.gl;
        let remap = !dev.clip_control;
        let programs = Programs {
            scene: program(gl, gl_src::SCENE_VERT, gl_src::SCENE_FRAG, remap)?,
            section: program(gl, gl_src::SCENE_VERT, gl_src::SECTION_FRAG, remap)?,
            xray: program(gl, gl_src::SCENE_VERT, gl_src::XRAY_FRAG, remap)?,
            edges: program(gl, gl_src::EDGES_VERT, gl_src::EDGES_FRAG, remap)?,
            pick: program(gl, gl_src::PICK_VERT, gl_src::PICK_FRAG, remap)?,
            background: program(gl, gl_src::BACKGROUND_VERT, gl_src::BACKGROUND_FRAG, false)?,
        };
        unsafe {
            let params = gl.create_buffer().map_err(GlError::Other)?;
            let push = gl.create_buffer().map_err(GlError::Other)?;
            let vao = gl.create_vertex_array().map_err(GlError::Other)?;
            let pick_target = PickTarget {
                fbo: gl.create_framebuffer().map_err(GlError::Other)?,
                ids: gl.create_texture().map_err(GlError::Other)?,
                depth: gl.create_texture().map_err(GlError::Other)?,
            };
            gl.bind_texture(glow::TEXTURE_2D, Some(pick_target.ids));
            gl.tex_storage_2d(glow::TEXTURE_2D, 1, glow::RG32UI, 1, 1);
            gl.bind_texture(glow::TEXTURE_2D, Some(pick_target.depth));
            gl.tex_storage_2d(glow::TEXTURE_2D, 1, glow::DEPTH_COMPONENT32F, 1, 1);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(pick_target.fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(pick_target.ids),
                0,
            );
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::TEXTURE_2D,
                Some(pick_target.depth),
                0,
            );
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            if status != glow::FRAMEBUFFER_COMPLETE {
                return Err(GlError::Other(format!("cible de sélection incomplète ({status:#x})")));
            }
            let s = GlScene {
                programs,
                geometry: None,
                params,
                push,
                pick_target,
                vao,
                section: RefCell::new(None),
                camera: RefCell::new(Camera::default()),
                env: env_texture(gl, &[(1, 1, vec![0, 0, 0, 0x3c00])])?,
            };
            upload(gl, s.params, glow::SHADER_STORAGE_BUFFER, &[0; 64], glow::DYNAMIC_DRAW);
            Ok(s)
        }
    }

    pub fn set_meshes(&mut self, dev: &GlDevice, meshes: &[MeshInput<'_>]) -> Result<()> {
        let gl = &dev.gl;
        self.free_geometry(gl);
        let Some(a) = GeometryArrays::new(meshes) else { return Ok(()) };
        let buf = |bytes: &[u8]| -> Result<glow::Buffer> {
            let b = unsafe { gl.create_buffer() }.map_err(GlError::Other)?;
            upload(gl, b, glow::SHADER_STORAGE_BUFFER, bytes, glow::STATIC_DRAW);
            Ok(b)
        };
        self.geometry = Some(Geometry {
            positions: buf(bytemuck::cast_slice(&a.positions))?,
            normals: buf(bytemuck::cast_slice(&a.normals))?,
            indices: buf(bytemuck::cast_slice(&a.indices))?,
            attrs: buf(bytemuck::cast_slice(&a.attrs))?,
            index_count: a.indices.len() as i32,
            edges: if a.edges.is_empty() {
                None
            } else {
                Some((buf(bytemuck::cast_slice(&a.edges))?, buf(bytemuck::cast_slice(&a.edge_parts))?))
            },
            edge_count: (a.edges.len() / 4) as i32,
            ranges: a.ranges,
        });
        Ok(())
    }

    fn free_geometry(&mut self, gl: &glow::Context) {
        if let Some(g) = self.geometry.take() {
            unsafe {
                for b in [g.positions, g.normals, g.indices, g.attrs] {
                    gl.delete_buffer(b);
                }
                if let Some((e, p)) = g.edges {
                    gl.delete_buffer(e);
                    gl.delete_buffer(p);
                }
            }
        }
    }

    /// Paramètres de l'image (œil, coupe, éclatement) pour les tracés suivants.
    pub fn set_params(&self, dev: &GlDevice, camera: &Camera, p: &SceneParams<'_>) {
        let ids: Vec<u32> = self.geometry.as_ref().map(|g| g.ranges.iter().map(|r| r.1).collect()).unwrap_or_default();
        upload(&dev.gl, self.params, glow::SHADER_STORAGE_BUFFER, &encode_params(camera, p, &ids), glow::DYNAMIC_DRAW);
        *self.section.borrow_mut() = p.section;
        *self.camera.borrow_mut() = *camera;
    }

    /// Remplace l'environnement d'éclairage (D-33).
    pub fn set_environment(&mut self, dev: &GlDevice, env: &obv_render::light::Environment) -> Result<()> {
        let gl = &dev.gl;
        let t = unsafe { env_texture(gl, &env.levels)? };
        unsafe { gl.delete_texture(self.env) };
        self.env = t;
        Ok(())
    }

    /// Environnement sur l'unité 1 ; l'unité 0 redevient active (l'interface y lie son atlas).
    fn bind_env(&self, gl: &glow::Context) {
        unsafe {
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.env));
            gl.active_texture(glow::TEXTURE0);
        }
    }

    fn bind_push(&self, gl: &glow::Context, bytes: &[u8]) {
        upload(gl, self.push, glow::UNIFORM_BUFFER, bytes, glow::STREAM_DRAW);
        unsafe { gl.bind_buffer_base(glow::UNIFORM_BUFFER, 0, Some(self.push)) };
    }

    fn bind_geometry(&self, gl: &glow::Context, g: &Geometry) {
        unsafe {
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer_base(glow::SHADER_STORAGE_BUFFER, 0, Some(g.positions));
            gl.bind_buffer_base(glow::SHADER_STORAGE_BUFFER, 1, Some(g.normals));
            gl.bind_buffer_base(glow::SHADER_STORAGE_BUFFER, 2, Some(g.indices));
            gl.bind_buffer_base(glow::SHADER_STORAGE_BUFFER, 3, Some(g.attrs));
            gl.bind_buffer_base(glow::SHADER_STORAGE_BUFFER, 4, Some(self.params));
        }
    }

    /// Fond en dégradé limité à `band`, sur le rectangle `rect` (pixels, haut à gauche) d'une cible haute de
    /// `height` pixels.
    pub fn draw_background_band(
        &self,
        dev: &GlDevice,
        rect: [f32; 4],
        height: u32,
        top: [f32; 4],
        bottom: [f32; 4],
        band: Band,
    ) {
        let gl = &dev.gl;
        let Some((w, h)) = set_rect(gl, rect, height) else { return };
        depth(gl, true, false, true);
        blend(gl, false);
        let inv_view_proj = obv_render::background_inverse(&self.camera.borrow(), (w / h) as f64);
        self.bind_push(gl, bytemuck::bytes_of(&BackgroundPush { top, bottom, band: band.raw(), inv_view_proj }));
        self.bind_env(gl);
        unsafe {
            gl.bind_buffer_base(glow::SHADER_STORAGE_BUFFER, 4, Some(self.params));
            gl.use_program(Some(self.programs.background));
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
    }

    /// Surfaces (et arêtes en rendu « Arêtes ») limitées à `band`.
    pub fn draw_scene(
        &self,
        dev: &GlDevice,
        rect: [f32; 4],
        height: u32,
        camera: &Camera,
        mode: RenderMode,
        band: Band,
    ) {
        let gl = &dev.gl;
        let Some((w, h)) = set_rect(gl, rect, height) else { return };
        let Some(g) = &self.geometry else { return };
        let view_proj = camera.view_proj((w / h) as f64);
        let sectioned = self.section.borrow().is_some();
        let (p, xray) = match mode {
            RenderMode::XRay => (self.programs.xray, true),
            _ if sectioned => (self.programs.section, false),
            _ => (self.programs.scene, false),
        };
        // Comme les pipelines Vulkan : rayons X sans test ni écriture de profondeur, mélange additif.
        depth(gl, true, !xray, xray);
        blend(gl, xray);
        self.bind_push(
            gl,
            bytemuck::bytes_of(&ScenePush { view_proj, mode: mode as u32, pad: [0; 3], band: band.raw() }),
        );
        self.bind_geometry(gl, g);
        self.bind_env(gl);
        unsafe {
            gl.use_program(Some(p));
            gl.draw_arrays(glow::TRIANGLES, 0, g.index_count);
        }
        if mode == RenderMode::Edges
            && let Some((e, parts)) = g.edges
        {
            depth(gl, true, false, false);
            blend(gl, true);
            self.bind_push(
                gl,
                bytemuck::bytes_of(&EdgePush {
                    view_proj,
                    viewport: [w, h],
                    width: 1.4,
                    color: u32::from_le_bytes([3, 4, 5, 255]),
                    band: band.raw(),
                }),
            );
            unsafe {
                gl.bind_buffer_base(glow::SHADER_STORAGE_BUFFER, 0, Some(e));
                gl.bind_buffer_base(glow::SHADER_STORAGE_BUFFER, 1, Some(parts));
                gl.bind_buffer_base(glow::SHADER_STORAGE_BUFFER, 4, Some(self.params));
                gl.use_program(Some(self.programs.edges));
                gl.draw_arrays(glow::TRIANGLES, 0, g.edge_count * 6);
            }
        }
    }

    /// Sélection au pixel `pixel` d'une vue `size` (pixels de la cible) : un tracé sur une cible de 1 × 1 par
    /// une projection resserrée sur ce pixel, comme sous Vulkan. Les paramètres de l'image doivent être à jour.
    pub fn pick(&self, dev: &GlDevice, camera: &Camera, size: [f32; 2], pixel: [f32; 2]) -> Option<Pick> {
        let gl = &dev.gl;
        let g = self.geometry.as_ref()?;
        if size[0] < 1.0 || size[1] < 1.0 {
            return None;
        }
        let aspect = (size[0] / size[1]) as f64;
        let (cx, cy) = obv_render::pick_ndc(size, pixel);
        let view_proj = camera.view_proj_region(aspect, [cx, cy], [size[0] as f64, size[1] as f64]);
        let t = &self.pick_target;
        let mut ids = [0u32; 2];
        let mut d = [0f32; 1];
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(t.fbo));
            gl.disable(glow::FRAMEBUFFER_SRGB);
            gl.disable(glow::SCISSOR_TEST);
            gl.viewport(0, 0, 1, 1);
            gl.depth_mask(true);
            gl.clear_buffer_u32_slice(glow::COLOR, 0, &[0, 0, 0, 0]);
            gl.clear_buffer_f32_slice(glow::DEPTH, 0, &[0.0]);
            depth(gl, true, true, false);
            blend(gl, false);
            self.bind_push(
                gl,
                bytemuck::bytes_of(&ScenePush { view_proj, mode: 0, pad: [0; 3], band: Band::ALL.raw() }),
            );
            self.bind_geometry(gl, g);
            gl.use_program(Some(self.programs.pick));
            gl.draw_arrays(glow::TRIANGLES, 0, g.index_count);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.read_buffer(glow::COLOR_ATTACHMENT0);
            gl.read_pixels(
                0,
                0,
                1,
                1,
                glow::RG_INTEGER,
                glow::UNSIGNED_INT,
                glow::PixelPackData::Slice(Some(bytemuck::cast_slice_mut(&mut ids))),
            );
            gl.read_pixels(
                0,
                0,
                1,
                1,
                glow::DEPTH_COMPONENT,
                glow::FLOAT,
                glow::PixelPackData::Slice(Some(bytemuck::cast_slice_mut(&mut d))),
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        let read = PickRead { triangle: ids[0], hit: ids[1], depth: d[0] as f64 };
        if std::env::var_os("OBV_GL_DEBUG").is_some() {
            eprintln!("sélection : {read:?}");
        }
        resolve_pick(camera, size, pixel, read, &g.ranges, *self.section.borrow())
    }

    pub fn destroy(mut self, dev: &GlDevice) {
        let gl = &dev.gl;
        self.free_geometry(gl);
        unsafe {
            let p = &self.programs;
            for prog in [p.scene, p.section, p.xray, p.edges, p.pick, p.background] {
                gl.delete_program(prog);
            }
            gl.delete_buffer(self.params);
            gl.delete_buffer(self.push);
            gl.delete_vertex_array(self.vao);
            gl.delete_framebuffer(self.pick_target.fbo);
            gl.delete_texture(self.pick_target.ids);
            gl.delete_texture(self.pick_target.depth);
            gl.delete_texture(self.env);
        }
    }
}
