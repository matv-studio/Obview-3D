//! Rendu de la scène 3D. Première étape : géométrie lue par adresse, un tracé, éclairage de présentation.
//! Étapes suivantes (D-08) : grappes, niveaux de détail, tri GPU, mesh shaders, arêtes, sélection.

use obv_gpu::{Buffer, Gpu, GraphicsPipeline, PipelineDesc, Result, ShaderStages, spirv, vk};

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

    /// Matrice vue × projection (colonnes), repère main droite, Y vers le haut.
    pub fn view_proj(&self, aspect: f64) -> [f32; 16] {
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
        let near = (self.distance * 1e-3).max(1e-6);
        let g = 1.0 / (self.fov_y * 0.5).tan();
        let proj = [g / aspect, 0.0, 0.0, 0.0, 0.0, g, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, near, 0.0];
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
    eye: [f32; 4],
    positions: u64,
    normals: u64,
    indices: u64,
    colors: u64,
}

/// Géométrie d'entrée du moteur de rendu.
pub struct MeshInput<'a> {
    pub positions: &'a [[f32; 3]],
    pub normals: &'a [[f32; 3]],
    pub indices: &'a [u32],
    /// Couleur sRGB 8 bits (R, G, B, A).
    pub color: [u8; 4],
}

struct Geometry {
    positions: Buffer,
    normals: Buffer,
    indices: Buffer,
    colors: Buffer,
    index_count: u32,
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
    geometry: Option<Geometry>,
}

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
        Ok(SceneRenderer { gpu: gpu.clone(), pipeline, background, geometry: None })
    }

    /// Remplace la géométrie affichée (tout est réuni dans un tampon par attribut).
    pub fn set_meshes(&mut self, meshes: &[MeshInput<'_>]) -> Result<()> {
        let mut pos = Vec::new();
        let mut nrm = Vec::new();
        let mut idx = Vec::new();
        let mut col = Vec::new();
        for m in meshes {
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
        });
        Ok(())
    }

    /// Dessine dans le rectangle `rect` (pixels de l'écran, origine en haut à gauche) de la cible courante :
    /// fond en dégradé (couleurs linéaires `top`, `bottom`), puis la géométrie.
    pub fn draw(&self, cmd: vk::CommandBuffer, rect: [f32; 4], camera: &Camera, top: [f32; 4], bottom: [f32; 4]) {
        let d = self.gpu.device();
        let (x, y, w, h) = (rect[0], rect[1], rect[2] - rect[0], rect[3] - rect[1]);
        if w < 1.0 || h < 1.0 {
            return;
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
        self.background.bind(cmd);
        self.background.push(cmd, &BackgroundPush { top, bottom });
        unsafe { d.cmd_draw(cmd, 3, 1, 0, 0) };
        let Some(g) = &self.geometry else { return };
        let eye = camera.eye();
        self.pipeline.bind(cmd);
        self.pipeline.push(
            cmd,
            &Push {
                view_proj: camera.view_proj((w / h) as f64),
                eye: [eye[0] as f32, eye[1] as f32, eye[2] as f32, 1.0],
                positions: g.positions.address,
                normals: g.normals.address,
                indices: g.indices.address,
                colors: g.colors.address,
            },
        );
        unsafe { d.cmd_draw(cmd, g.index_count, 1, 0, 0) };
    }
}
