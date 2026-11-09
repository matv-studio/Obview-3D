//! Rendu de la scène 3D. Première étape : géométrie lue par adresse, un tracé, éclairage de présentation.
//! Étapes suivantes (D-08) : grappes, niveaux de détail, tri GPU, mesh shaders, arêtes, sélection.

mod edges;

pub use edges::feature_edges;
use obv_gpu::{Buffer, Gpu, GraphicsPipeline, Image, MemoryLocation, PipelineDesc, Result, ShaderStages, spirv, vk};

/// Rendus de la vue 3D (D-15).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderMode {
    Lit = 0,
    Color = 1,
    Normals = 2,
    Topology = 3,
    XRay = 4,
    Edges = 5,
}

impl RenderMode {
    pub const ALL: [RenderMode; 6] = [
        RenderMode::Lit,
        RenderMode::Color,
        RenderMode::Normals,
        RenderMode::Topology,
        RenderMode::XRay,
        RenderMode::Edges,
    ];
}

/// Bande de l'inspecteur : fragments gardés si `lo <= dot(pixel, normal) < hi` (pixels de la cible).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub normal: [f32; 2],
    pub lo: f32,
    pub hi: f32,
}

impl Band {
    /// Toute la vue.
    pub const ALL: Band = Band { normal: [0.0, 0.0], lo: -1.0, hi: 1.0 };
    fn raw(self) -> [f32; 4] {
        [self.normal[0], self.normal[1], self.lo, self.hi]
    }
}

/// Caméra en orbite autour d'une cible, profondeur inversée à plan lointain infini.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub target: [f64; 3],
    pub distance: f64,
    /// Rotation autour de l'axe vertical (radians).
    pub yaw: f64,
    /// Inclinaison (radians, positive : vue du dessus).
    pub pitch: f64,
    pub fov_y: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Camera { target: [0.0; 3], distance: 5.0, yaw: 0.6, pitch: 0.45, fov_y: 35f64.to_radians() }
    }
}

impl Camera {
    pub fn eye(&self) -> [f64; 3] {
        let (cy, sy, cp, sp) = (self.yaw.cos(), self.yaw.sin(), self.pitch.cos(), self.pitch.sin());
        let d = self.distance;
        [self.target[0] + d * cp * sy, self.target[1] + d * sp, self.target[2] + d * cp * cy]
    }

    /// Cadre la boîte englobante (sphère englobante entièrement visible, avec une marge).
    pub fn frame(&mut self, lo: [f32; 3], hi: [f32; 3], aspect: f64) {
        let c = [(lo[0] + hi[0]) as f64 * 0.5, (lo[1] + hi[1]) as f64 * 0.5, (lo[2] + hi[2]) as f64 * 0.5];
        let r = ((hi[0] - lo[0]) as f64).hypot((hi[1] - lo[1]) as f64).hypot((hi[2] - lo[2]) as f64) * 0.5;
        let half_v = self.fov_y * 0.5;
        let half_h = (half_v.tan() * aspect.max(1e-3)).atan();
        self.target = c;
        self.distance = r / half_v.min(half_h).sin() * 1.05;
    }

    /// Plan proche de la projection (profondeur inversée : profondeur = proche / distance de vue).
    pub fn near(&self) -> f64 {
        (self.distance * 1e-3).max(1e-6)
    }

    /// Matrice vue × projection (colonnes), repère main droite, Y vers le haut.
    pub fn view_proj(&self, aspect: f64) -> [f32; 16] {
        self.view_proj_region(aspect, [0.0, 0.0], [1.0, 1.0])
    }

    /// Comme [`Self::view_proj`], restreinte à une région de l'écran : `center` en coordonnées normalisées
    /// (-1 à 1, Y vers le haut), `scale` agrandissement (taille de la vue / taille de la région).
    pub fn view_proj_region(&self, aspect: f64, center: [f64; 2], scale: [f64; 2]) -> [f32; 16] {
        let eye = self.eye();
        let t = self.target;
        let f = norm([t[0] - eye[0], t[1] - eye[1], t[2] - eye[2]]);
        let s = norm(cross(f, [0.0, 1.0, 0.0]));
        let u = cross(s, f);
        let view = [
            s[0],
            u[0],
            -f[0],
            0.0,
            s[1],
            u[1],
            -f[1],
            0.0,
            s[2],
            u[2],
            -f[2],
            0.0,
            -dot(s, eye),
            -dot(u, eye),
            dot(f, eye),
            1.0,
        ];
        let near = self.near();
        let g = 1.0 / (self.fov_y * 0.5).tan();
        let mut proj = [g / aspect, 0.0, 0.0, 0.0, 0.0, g, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, near, 0.0];
        // Région : x' = s (x - c w), appliqué aux lignes x et y de la projection (w = -z_vue).
        for c in 0..4 {
            proj[c * 4] = scale[0] * (proj[c * 4] - center[0] * proj[c * 4 + 3]);
            proj[c * 4 + 1] = scale[1] * (proj[c * 4 + 1] - center[1] * proj[c * 4 + 3]);
        }
        let mut r = [0f32; 16];
        for c in 0..4 {
            for row in 0..4 {
                r[c * 4 + row] = (0..4).map(|k| proj[k * 4 + row] * view[c * 4 + k]).sum::<f64>() as f32;
            }
        }
        r
    }
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: [f64; 3]) -> [f64; 3] {
    let l = dot(a, a).sqrt().max(1e-300);
    [a[0] / l, a[1] / l, a[2] / l]
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Push {
    view_proj: [f32; 16],
    eye: [f32; 3],
    mode: u32,
    positions: u64,
    normals: u64,
    indices: u64,
    colors: u64,
    band: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EdgePush {
    view_proj: [f32; 16],
    edges: u64,
    viewport: [f32; 2],
    width: f32,
    /// Couleur linéaire non prémultipliée, 8 bits par canal : R, G, B, A.
    color: u32,
    eye: [f32; 3],
    pad: f32,
    band: [f32; 4],
}

/// Géométrie d'entrée du moteur de rendu.
pub struct MeshInput<'a> {
    pub positions: &'a [[f32; 3]],
    pub normals: &'a [[f32; 3]],
    pub indices: &'a [u32],
    /// Couleur sRGB 8 bits (R, G, B, A).
    pub color: [u8; 4],
    /// Identifiant rendu par la sélection au pixel (indice de la pièce).
    pub id: u32,
    /// Arêtes à tracer (groupes de quatre points), voir [`feature_edges`].
    pub edges: &'a [[f32; 3]],
}

/// Résultat de la sélection au pixel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pick {
    /// Identifiant de la géométrie (`MeshInput::id`).
    pub id: u32,
    /// Triangle dans cette géométrie.
    pub triangle: u32,
    /// Point de la surface, coordonnées du monde.
    pub position: [f64; 3],
}

struct PickTarget {
    ids: Image,
    depth: Image,
    readback: Buffer,
}

struct Geometry {
    positions: Buffer,
    normals: Buffer,
    indices: Buffer,
    colors: Buffer,
    index_count: u32,
    edges: Option<Buffer>,
    edge_count: u32,
    /// Premier triangle de chaque géométrie et son identifiant, dans l'ordre du tracé.
    ranges: Vec<(u32, u32)>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BackgroundPush {
    top: [f32; 4],
    bottom: [f32; 4],
}

pub struct SceneRenderer {
    gpu: Gpu,
    pipeline: GraphicsPipeline,
    background: GraphicsPipeline,
    xray: GraphicsPipeline,
    edges: GraphicsPipeline,
    pick_pipeline: GraphicsPipeline,
    pick_target: PickTarget,
    geometry: Option<Geometry>,
}

const PICK_FORMAT: vk::Format = vk::Format::R32G32_UINT;

impl SceneRenderer {
    pub fn new(gpu: &Gpu, color_format: vk::Format, depth_format: vk::Format) -> Result<Self> {
        let pipeline = GraphicsPipeline::new(
            gpu,
            &PipelineDesc {
                stages: ShaderStages::Vertex { vertex: spirv!("scene.vert"), fragment: spirv!("scene.frag") },
                color_formats: &[color_format],
                depth_format: Some(depth_format),
                cull_mode: vk::CullModeFlags::NONE,
                push_constant_size: std::mem::size_of::<Push>() as u32,
                ..Default::default()
            },
        )?;
        let background = GraphicsPipeline::new(
            gpu,
            &PipelineDesc {
                stages: ShaderStages::Vertex { vertex: spirv!("background.vert"), fragment: spirv!("background.frag") },
                color_formats: &[color_format],
                depth_format: Some(depth_format),
                depth_write: false,
                depth_compare: vk::CompareOp::ALWAYS,
                cull_mode: vk::CullModeFlags::NONE,
                push_constant_size: std::mem::size_of::<BackgroundPush>() as u32,
                ..Default::default()
            },
        )?;
        let xray = GraphicsPipeline::new(
            gpu,
            &PipelineDesc {
                stages: ShaderStages::Vertex { vertex: spirv!("scene.vert"), fragment: spirv!("xray.frag") },
                color_formats: &[color_format],
                depth_format: Some(depth_format),
                depth_write: false,
                depth_compare: vk::CompareOp::ALWAYS,
                cull_mode: vk::CullModeFlags::NONE,
                alpha_blend: true,
                push_constant_size: std::mem::size_of::<Push>() as u32,
                ..Default::default()
            },
        )?;
        let edges = GraphicsPipeline::new(
            gpu,
            &PipelineDesc {
                stages: ShaderStages::Vertex { vertex: spirv!("edges.vert"), fragment: spirv!("edges.frag") },
                color_formats: &[color_format],
                depth_format: Some(depth_format),
                depth_write: false,
                cull_mode: vk::CullModeFlags::NONE,
                alpha_blend: true,
                push_constant_size: std::mem::size_of::<EdgePush>() as u32,
                ..Default::default()
            },
        )?;
        let pick_pipeline = GraphicsPipeline::new(
            gpu,
            &PipelineDesc {
                stages: ShaderStages::Vertex { vertex: spirv!("pick.vert"), fragment: spirv!("pick.frag") },
                color_formats: &[PICK_FORMAT],
                depth_format: Some(obv_gpu::RenderTarget::DEPTH_FORMAT),
                cull_mode: vk::CullModeFlags::NONE,
                push_constant_size: std::mem::size_of::<Push>() as u32,
                ..Default::default()
            },
        )?;
        let one = vk::Extent2D { width: 1, height: 1 };
        let pick_target = PickTarget {
            ids: Image::new(
                gpu,
                "sélection",
                PICK_FORMAT,
                one,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
                vk::ImageAspectFlags::COLOR,
            )?,
            depth: Image::new(
                gpu,
                "sélection profondeur",
                obv_gpu::RenderTarget::DEPTH_FORMAT,
                one,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
                vk::ImageAspectFlags::DEPTH,
            )?,
            readback: Buffer::new(
                gpu,
                "relecture sélection",
                16,
                vk::BufferUsageFlags::TRANSFER_DST,
                MemoryLocation::GpuToCpu,
            )?,
        };
        Ok(SceneRenderer {
            gpu: gpu.clone(),
            pipeline,
            background,
            xray,
            edges,
            pick_pipeline,
            pick_target,
            geometry: None,
        })
    }

    /// Remplace la géométrie affichée (tout est réuni dans un tampon par attribut).
    pub fn set_meshes(&mut self, meshes: &[MeshInput<'_>]) -> Result<()> {
        let mut pos = Vec::new();
        let mut nrm = Vec::new();
        let mut idx = Vec::new();
        let mut col = Vec::new();
        let mut ranges = Vec::with_capacity(meshes.len());
        let mut segs: Vec<[f32; 3]> = Vec::new();
        for m in meshes {
            segs.extend_from_slice(m.edges);
            ranges.push(((idx.len() / 3) as u32, m.id));
            let base = pos.len() as u32;
            pos.extend_from_slice(m.positions);
            nrm.extend_from_slice(m.normals);
            col.extend(std::iter::repeat_n(u32::from_le_bytes(m.color), m.positions.len()));
            idx.extend(m.indices.iter().map(|i| i + base));
        }
        if idx.is_empty() {
            self.geometry = None;
            return Ok(());
        }
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER;
        self.geometry = Some(Geometry {
            positions: Buffer::with_data(&self.gpu, "positions", bytemuck::cast_slice(&pos), usage)?,
            normals: Buffer::with_data(&self.gpu, "normales", bytemuck::cast_slice(&nrm), usage)?,
            indices: Buffer::with_data(&self.gpu, "indices", bytemuck::cast_slice(&idx), usage)?,
            colors: Buffer::with_data(&self.gpu, "couleurs", bytemuck::cast_slice(&col), usage)?,
            index_count: idx.len() as u32,
            edges: if segs.is_empty() {
                None
            } else {
                Some(Buffer::with_data(&self.gpu, "arêtes", bytemuck::cast_slice(&segs), usage)?)
            },
            edge_count: (segs.len() / 4) as u32,
            ranges,
        });
        Ok(())
    }

    fn set_rect(&self, cmd: vk::CommandBuffer, rect: [f32; 4]) -> Option<(f32, f32)> {
        let d = self.gpu.device();
        let (x, y, w, h) = (rect[0], rect[1], rect[2] - rect[0], rect[3] - rect[1]);
        if w < 1.0 || h < 1.0 {
            return None;
        }
        unsafe {
            d.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport { x, y: y + h, width: w, height: -h, min_depth: 0.0, max_depth: 1.0 }],
            );
            d.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D {
                    offset: vk::Offset2D { x: x as i32, y: y as i32 },
                    extent: vk::Extent2D { width: w as u32, height: h as u32 },
                }],
            );
        }
        Some((w, h))
    }

    /// Fond de la vue : dégradé vertical (couleurs linéaires `top`, `bottom`) dans `rect` (pixels de la cible).
    pub fn draw_background(&self, cmd: vk::CommandBuffer, rect: [f32; 4], top: [f32; 4], bottom: [f32; 4]) {
        if self.set_rect(cmd, rect).is_none() {
            return;
        }
        self.background.bind(cmd);
        self.background.push(cmd, &BackgroundPush { top, bottom });
        unsafe { self.gpu.device().cmd_draw(cmd, 3, 1, 0, 0) };
    }

    /// Géométrie dans `rect` avec le rendu `mode`, limitée à la bande `band`.
    pub fn draw_scene(&self, cmd: vk::CommandBuffer, rect: [f32; 4], camera: &Camera, mode: RenderMode, band: Band) {
        let Some((w, h)) = self.set_rect(cmd, rect) else { return };
        let Some(g) = &self.geometry else { return };
        let d = self.gpu.device();
        let eye = camera.eye();
        let view_proj = camera.view_proj((w / h) as f64);
        let push = Push {
            view_proj,
            eye: [eye[0] as f32, eye[1] as f32, eye[2] as f32],
            mode: mode as u32,
            positions: g.positions.address,
            normals: g.normals.address,
            indices: g.indices.address,
            colors: g.colors.address,
            band: band.raw(),
        };
        let p = if mode == RenderMode::XRay { &self.xray } else { &self.pipeline };
        p.bind(cmd);
        p.push(cmd, &push);
        unsafe { d.cmd_draw(cmd, g.index_count, 1, 0, 0) };
        if mode == RenderMode::Edges
            && let Some(e) = &g.edges
        {
            self.edges.bind(cmd);
            self.edges.push(
                cmd,
                &EdgePush {
                    view_proj,
                    edges: e.address,
                    viewport: [w, h],
                    width: 1.4,
                    // Linéaire (0,012 ; 0,014 ; 0,018), opaque.
                    color: u32::from_le_bytes([3, 4, 5, 255]),
                    eye: [eye[0] as f32, eye[1] as f32, eye[2] as f32],
                    pad: 0.0,
                    band: band.raw(),
                },
            );
            unsafe { d.cmd_draw(cmd, g.edge_count * 6, 1, 0, 0) };
        }
    }

    /// Fond en dégradé puis géométrie éclairée (vue normale).
    pub fn draw(&self, cmd: vk::CommandBuffer, rect: [f32; 4], camera: &Camera, top: [f32; 4], bottom: [f32; 4]) {
        self.draw_background(cmd, rect, top, bottom);
        self.draw_scene(cmd, rect, camera, RenderMode::Lit, Band::ALL);
    }

    /// Sélection au pixel : rend la scène dans une cible d'un pixel centrée sur `pixel` (pixels de l'écran,
    /// relatifs au coin haut gauche de la vue de taille `size`), puis relit le triangle et la profondeur.
    pub fn pick(&self, camera: &Camera, size: [f32; 2], pixel: [f32; 2]) -> Result<Option<Pick>> {
        let Some(g) = &self.geometry else { return Ok(None) };
        if size[0] < 1.0 || size[1] < 1.0 {
            return Ok(None);
        }
        let aspect = (size[0] / size[1]) as f64;
        let cx = 2.0 * (pixel[0] as f64 + 0.5) / size[0] as f64 - 1.0;
        let cy = 1.0 - 2.0 * (pixel[1] as f64 + 0.5) / size[1] as f64;
        let view_proj = camera.view_proj_region(aspect, [cx, cy], [size[0] as f64, size[1] as f64]);
        let eye = camera.eye();
        let t = &self.pick_target;
        let d = self.gpu.device().clone();
        let push = Push {
            view_proj,
            eye: [eye[0] as f32, eye[1] as f32, eye[2] as f32],
            mode: 0,
            positions: g.positions.address,
            normals: g.normals.address,
            indices: g.indices.address,
            colors: g.colors.address,
            band: Band::ALL.raw(),
        };
        self.gpu.submit_and_wait(|cmd| unsafe {
            let none = (vk::PipelineStageFlags2::TOP_OF_PIPE, vk::AccessFlags2::NONE, vk::ImageLayout::UNDEFINED);
            t.ids.barrier(
                cmd,
                none,
                (
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ),
            );
            let ds = vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS;
            t.depth.barrier(
                cmd,
                none,
                (
                    ds,
                    vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ,
                    vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
                ),
            );
            let color = [vk::RenderingAttachmentInfo::default()
                .image_view(t.ids.view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue { color: vk::ClearColorValue { uint32: [0; 4] } })];
            let depth = vk::RenderingAttachmentInfo::default()
                .image_view(t.depth.view)
                .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 0.0, stencil: 0 } });
            let area = vk::Rect2D { offset: vk::Offset2D::default(), extent: vk::Extent2D { width: 1, height: 1 } };
            d.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(area)
                    .layer_count(1)
                    .color_attachments(&color)
                    .depth_attachment(&depth),
            );
            d.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport { x: 0.0, y: 1.0, width: 1.0, height: -1.0, min_depth: 0.0, max_depth: 1.0 }],
            );
            d.cmd_set_scissor(cmd, 0, &[area]);
            self.pick_pipeline.bind(cmd);
            self.pick_pipeline.push(cmd, &push);
            d.cmd_draw(cmd, g.index_count, 1, 0, 0);
            d.cmd_end_rendering(cmd);
            t.ids.barrier(
                cmd,
                (
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ),
                (vk::PipelineStageFlags2::COPY, vk::AccessFlags2::TRANSFER_READ, vk::ImageLayout::TRANSFER_SRC_OPTIMAL),
            );
            t.depth.barrier(
                cmd,
                (ds, vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE, vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL),
                (vk::PipelineStageFlags2::COPY, vk::AccessFlags2::TRANSFER_READ, vk::ImageLayout::TRANSFER_SRC_OPTIMAL),
            );
            let region = |aspect, offset| {
                vk::BufferImageCopy::default()
                    .buffer_offset(offset)
                    .image_subresource(vk::ImageSubresourceLayers::default().aspect_mask(aspect).layer_count(1))
                    .image_extent(vk::Extent3D { width: 1, height: 1, depth: 1 })
            };
            d.cmd_copy_image_to_buffer(
                cmd,
                t.ids.raw,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                t.readback.raw,
                &[region(vk::ImageAspectFlags::COLOR, 0)],
            );
            d.cmd_copy_image_to_buffer(
                cmd,
                t.depth.raw,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                t.readback.raw,
                &[region(vk::ImageAspectFlags::DEPTH, 8)],
            );
            let host = [vk::BufferMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COPY)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::HOST)
                .dst_access_mask(vk::AccessFlags2::HOST_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(t.readback.raw)
                .size(vk::WHOLE_SIZE)];
            d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().buffer_memory_barriers(&host));
        })?;
        let bytes = t.readback.mapped().expect("tampon de relecture non projeté");
        let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        let (triangle, hit, depth) = (word(0), word(1), f32::from_bits(word(2)) as f64);
        if hit == 0 || depth <= 0.0 {
            return Ok(None);
        }
        let k = g.ranges.partition_point(|(first, _)| *first <= triangle).saturating_sub(1);
        let (first, id) = g.ranges[k];
        // Profondeur inversée : distance de vue = proche / profondeur ; point sur le rayon du pixel.
        let view_depth = camera.near() / depth;
        let (cy_, sy_, cp, sp) = (camera.yaw.cos(), camera.yaw.sin(), camera.pitch.cos(), camera.pitch.sin());
        let back = [cp * sy_, sp, cp * cy_];
        let right = [cy_, 0.0, -sy_];
        let up = [-sp * sy_, cp, -sp * cy_];
        let half_h = (camera.fov_y * 0.5).tan();
        let dir: [f64; 3] = std::array::from_fn(|i| right[i] * cx * half_h * aspect + up[i] * cy * half_h - back[i]);
        let position = std::array::from_fn(|i| eye[i] + dir[i] * view_depth);
        Ok(Some(Pick { id, triangle: triangle - first, position }))
    }
}
