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

/// Caméra en orbite autour d'une cible. Perspective : profondeur inversée à plan lointain infini.
/// Orthographique : même taille visible au plan de la cible (demi-hauteur = distance × tan(fov/2)), profondeur
/// inversée linéaire sur la scène entière.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub target: [f64; 3],
    pub distance: f64,
    /// Rotation autour de l'axe vertical (radians).
    pub yaw: f64,
    /// Inclinaison (radians, positive : vue du dessus).
    pub pitch: f64,
    pub fov_y: f64,
    /// Projection orthographique.
    pub ortho: bool,
    /// Rayon de la scène (bornes de profondeur de la projection orthographique).
    pub radius: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            target: [0.0; 3],
            distance: 5.0,
            yaw: 0.6,
            pitch: 0.45,
            fov_y: 35f64.to_radians(),
            ortho: false,
            radius: 1.0,
        }
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
        self.radius = r.max(1e-9);
    }

    /// Direction de visée (de l'œil vers la cible), unitaire.
    pub fn forward(&self) -> [f64; 3] {
        let (cy, sy, cp, sp) = (self.yaw.cos(), self.yaw.sin(), self.pitch.cos(), self.pitch.sin());
        [-cp * sy, -sp, -cp * cy]
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
        self.matrix(aspect, center, scale).map(|v| v as f32)
    }

    /// Matrice vue × projection en double précision (colonnes).
    pub fn matrix(&self, aspect: f64, center: [f64; 2], scale: [f64; 2]) -> [f64; 16] {
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
        let mut proj = if self.ortho {
            // Profondeur inversée linéaire : z de vue de `back` (derrière l'œil) à `-far` → 1 à 0.
            let h = self.distance * (self.fov_y * 0.5).tan();
            let back = 2.0 * self.radius;
            let far = self.distance + 2.0 * self.radius;
            let k = 1.0 / (back + far);
            [1.0 / (h * aspect), 0.0, 0.0, 0.0, 0.0, 1.0 / h, 0.0, 0.0, 0.0, 0.0, k, 0.0, 0.0, 0.0, far * k, 1.0]
        } else {
            let near = self.near();
            let g = 1.0 / (self.fov_y * 0.5).tan();
            [g / aspect, 0.0, 0.0, 0.0, 0.0, g, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, near, 0.0]
        };
        // Région : x' = s (x - c w), appliqué aux lignes x et y de la projection.
        for c in 0..4 {
            proj[c * 4] = scale[0] * (proj[c * 4] - center[0] * proj[c * 4 + 3]);
            proj[c * 4 + 1] = scale[1] * (proj[c * 4 + 1] - center[1] * proj[c * 4 + 3]);
        }
        let mut r = [0f64; 16];
        for c in 0..4 {
            for row in 0..4 {
                r[c * 4 + row] = (0..4).map(|k| proj[k * 4 + row] * view[c * 4 + k]).sum::<f64>();
            }
        }
        r
    }

    /// Point du monde sous les coordonnées normalisées `ndc` (x, y de -1 à 1, profondeur de la cible).
    pub fn unproject(&self, aspect: f64, ndc: [f64; 3]) -> Option<[f64; 3]> {
        let inv = invert(&self.matrix(aspect, [0.0, 0.0], [1.0, 1.0]))?;
        let v = [ndc[0], ndc[1], ndc[2], 1.0];
        let mut o = [0f64; 4];
        for (row, out) in o.iter_mut().enumerate() {
            *out = (0..4).map(|k| inv[k * 4 + row] * v[k]).sum();
        }
        (o[3].abs() > 1e-300).then(|| [o[0] / o[3], o[1] / o[3], o[2] / o[3]])
    }

    /// Projette un point du monde : coordonnées normalisées (x, y) et vrai s'il est devant l'œil.
    pub fn project(&self, aspect: f64, p: [f64; 3]) -> ([f64; 2], bool) {
        let m = self.matrix(aspect, [0.0, 0.0], [1.0, 1.0]);
        let c: [f64; 4] = std::array::from_fn(|row| (0..3).map(|k| m[k * 4 + row] * p[k]).sum::<f64>() + m[12 + row]);
        let w = if c[3].abs() < 1e-300 { 1e-300 } else { c[3] };
        ([c[0] / w, c[1] / w], c[3] > 0.0)
    }
}

/// Inverse d'une matrice 4 × 4 (colonnes), par cofacteurs.
fn invert(m: &[f64; 16]) -> Option<[f64; 16]> {
    let mut inv = [0f64; 16];
    inv[0] =
        m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15] + m[9] * m[7] * m[14] + m[13] * m[6] * m[11]
            - m[13] * m[7] * m[10];
    inv[4] =
        -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15] - m[8] * m[7] * m[14] - m[12] * m[6] * m[11]
            + m[12] * m[7] * m[10];
    inv[8] =
        m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15] + m[8] * m[7] * m[13] + m[12] * m[5] * m[11]
            - m[12] * m[7] * m[9];
    inv[12] =
        -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14] - m[8] * m[6] * m[13] - m[12] * m[5] * m[10]
            + m[12] * m[6] * m[9];
    inv[1] =
        -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15] - m[9] * m[3] * m[14] - m[13] * m[2] * m[11]
            + m[13] * m[3] * m[10];
    inv[5] =
        m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15] + m[8] * m[3] * m[14] + m[12] * m[2] * m[11]
            - m[12] * m[3] * m[10];
    inv[9] =
        -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15] - m[8] * m[3] * m[13] - m[12] * m[1] * m[11]
            + m[12] * m[3] * m[9];
    inv[13] =
        m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14] + m[8] * m[2] * m[13] + m[12] * m[1] * m[10]
            - m[12] * m[2] * m[9];
    inv[2] =
        m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15] + m[5] * m[3] * m[14] + m[13] * m[2] * m[7]
            - m[13] * m[3] * m[6];
    inv[6] =
        -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15] - m[4] * m[3] * m[14] - m[12] * m[2] * m[7]
            + m[12] * m[3] * m[6];
    inv[10] =
        m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15] + m[4] * m[3] * m[13] + m[12] * m[1] * m[7]
            - m[12] * m[3] * m[5];
    inv[14] =
        -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14] - m[4] * m[2] * m[13] - m[12] * m[1] * m[6]
            + m[12] * m[2] * m[5];
    inv[3] =
        -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11] - m[5] * m[3] * m[10] - m[9] * m[2] * m[7]
            + m[9] * m[3] * m[6];
    inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11] + m[4] * m[3] * m[10] + m[8] * m[2] * m[7]
        - m[8] * m[3] * m[6];
    inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11] - m[4] * m[3] * m[9] - m[8] * m[1] * m[7]
        + m[8] * m[3] * m[5];
    inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10] + m[4] * m[2] * m[9] + m[8] * m[1] * m[6]
        - m[8] * m[2] * m[5];
    let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    if det.abs() < 1e-300 {
        return None;
    }
    Some(inv.map(|v| v / det))
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
    positions: u64,
    normals: u64,
    indices: u64,
    attrs: u64,
    params: u64,
    mode: u32,
    pad: u32,
    band: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EdgePush {
    view_proj: [f32; 16],
    edges: u64,
    parts: u64,
    params: u64,
    viewport: [f32; 2],
    width: f32,
    /// Couleur linéaire non prémultipliée, 8 bits par canal : R, G, B, A.
    color: u32,
    band: [f32; 4],
}

/// En-tête des paramètres de l'image (voir `scene_common.glsl`), suivi d'un décalage par pièce.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ParamsHeader {
    eye: [f32; 3],
    ortho: u32,
    forward: [f32; 3],
    flags: u32,
    plane: [f32; 4],
}

/// Réglages de la scène pour une image : coupe et vue éclatée.
#[derive(Clone, Copy, Debug, Default)]
pub struct SceneParams<'a> {
    /// Plan de coupe (normale unitaire, distance) : on garde `dot(n, p) <= d`, faces coupées bouchées.
    pub section: Option<[f32; 4]>,
    /// Décalage de chaque pièce, indexé par `MeshInput::id` (vide : aucun).
    pub offsets: &'a [[f32; 3]],
}

/// Anneau de tampons de paramètres : une image en cours d'exécution n'est jamais réécrite.
const PARAM_SLOTS: usize = 4;

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
    /// Par sommet : couleur, rang de la géométrie dans le tracé.
    attrs: Buffer,
    index_count: u32,
    edges: Option<(Buffer, Buffer)>,
    edge_count: u32,
    /// Premier triangle de chaque géométrie et son identifiant, dans l'ordre du tracé.
    ranges: Vec<(u32, u32)>,
}

/// Tampons de paramètres (anneau, plus un pour la sélection au pixel) et dernier réglage reçu.
struct ParamRing {
    slots: Vec<Buffer>,
    pick: Buffer,
    next: usize,
    current: u64,
    section: Option<[f32; 4]>,
    /// Décalages dans l'ordre du tracé.
    offsets: Vec<[f32; 4]>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BackgroundPush {
    top: [f32; 4],
    bottom: [f32; 4],
    /// Bande gardée (comme pour la scène).
    band: [f32; 4],
}

pub struct SceneRenderer {
    gpu: Gpu,
    pipeline: GraphicsPipeline,
    background: GraphicsPipeline,
    xray: GraphicsPipeline,
    edges: GraphicsPipeline,
    pick_pipeline: GraphicsPipeline,
    section: GraphicsPipeline,
    pick_target: PickTarget,
    geometry: Option<Geometry>,
    params: std::cell::RefCell<ParamRing>,
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
        let section = GraphicsPipeline::new(
            gpu,
            &PipelineDesc {
                stages: ShaderStages::Vertex { vertex: spirv!("scene.vert"), fragment: spirv!("scene_section.frag") },
                color_formats: &[color_format],
                depth_format: Some(depth_format),
                cull_mode: vk::CullModeFlags::NONE,
                push_constant_size: std::mem::size_of::<Push>() as u32,
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
            section,
            pick_target,
            geometry: None,
            params: std::cell::RefCell::new(ParamRing::new(gpu, 1)?),
        })
    }

    /// Paramètres de l'image (œil, projection, coupe, éclatement), à donner avant les tracés de chaque image.
    pub fn set_params(&self, camera: &Camera, p: &SceneParams<'_>) -> Result<()> {
        let ids: Vec<u32> = self.geometry.as_ref().map(|g| g.ranges.iter().map(|r| r.1).collect()).unwrap_or_default();
        let offsets: Vec<[f32; 4]> = ids
            .iter()
            .map(|&id| p.offsets.get(id as usize).map(|o| [o[0], o[1], o[2], 0.0]).unwrap_or([0.0; 4]))
            .collect();
        let mut ring = self.params.borrow_mut();
        ring.section = p.section;
        ring.offsets = offsets;
        let bytes = ring.encode(camera);
        let k = ring.next;
        ring.next = (k + 1) % PARAM_SLOTS;
        if ring.slots[k].size < bytes.len() as u64 {
            ring.slots[k] = ParamRing::slot(&self.gpu, bytes.len())?;
        }
        ring.slots[k].mapped_mut().expect("paramètres non projetés")[..bytes.len()].copy_from_slice(&bytes);
        ring.current = ring.slots[k].address;
        Ok(())
    }

    fn current_params(&self) -> u64 {
        self.params.borrow().current
    }

    /// Remplace la géométrie affichée (tout est réuni dans un tampon par attribut). Les paramètres de l'image
    /// sont remis à zéro (sans coupe ni décalage) jusqu'au prochain [`Self::set_params`].
    pub fn set_meshes(&mut self, meshes: &[MeshInput<'_>]) -> Result<()> {
        let mut pos = Vec::new();
        let mut nrm = Vec::new();
        let mut idx = Vec::new();
        let mut attrs: Vec<[u32; 2]> = Vec::new();
        let mut ranges = Vec::with_capacity(meshes.len());
        let mut segs: Vec<[f32; 3]> = Vec::new();
        let mut seg_parts: Vec<u32> = Vec::new();
        for (k, m) in meshes.iter().enumerate() {
            segs.extend_from_slice(m.edges);
            seg_parts.extend(std::iter::repeat_n(k as u32, m.edges.len() / 4));
            ranges.push(((idx.len() / 3) as u32, m.id));
            let base = pos.len() as u32;
            pos.extend_from_slice(m.positions);
            nrm.extend_from_slice(m.normals);
            attrs.extend(std::iter::repeat_n([u32::from_le_bytes(m.color), k as u32], m.positions.len()));
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
            attrs: Buffer::with_data(&self.gpu, "attributs", bytemuck::cast_slice(&attrs), usage)?,
            index_count: idx.len() as u32,
            edges: if segs.is_empty() {
                None
            } else {
                Some((
                    Buffer::with_data(&self.gpu, "arêtes", bytemuck::cast_slice(&segs), usage)?,
                    Buffer::with_data(&self.gpu, "pièces des arêtes", bytemuck::cast_slice(&seg_parts), usage)?,
                ))
            },
            edge_count: (segs.len() / 4) as u32,
            ranges,
        });
        self.set_params(&Camera::default(), &SceneParams::default())
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
        self.draw_background_band(cmd, rect, top, bottom, Band::ALL);
    }

    /// Fond limité à la bande `band` (transitions de l'inspecteur).
    pub fn draw_background_band(
        &self,
        cmd: vk::CommandBuffer,
        rect: [f32; 4],
        top: [f32; 4],
        bottom: [f32; 4],
        band: Band,
    ) {
        if self.set_rect(cmd, rect).is_none() {
            return;
        }
        self.background.bind(cmd);
        self.background.push(cmd, &BackgroundPush { top, bottom, band: band.raw() });
        unsafe { self.gpu.device().cmd_draw(cmd, 3, 1, 0, 0) };
    }

    /// Géométrie dans `rect` avec le rendu `mode`, limitée à la bande `band`.
    pub fn draw_scene(&self, cmd: vk::CommandBuffer, rect: [f32; 4], camera: &Camera, mode: RenderMode, band: Band) {
        let Some((w, h)) = self.set_rect(cmd, rect) else { return };
        let Some(g) = &self.geometry else { return };
        let d = self.gpu.device();
        let view_proj = camera.view_proj((w / h) as f64);
        let params = self.current_params();
        let sectioned = self.params.borrow().section.is_some();
        let push = Push {
            view_proj,
            positions: g.positions.address,
            normals: g.normals.address,
            indices: g.indices.address,
            attrs: g.attrs.address,
            params,
            mode: mode as u32,
            pad: 0,
            band: band.raw(),
        };
        let p = match mode {
            RenderMode::XRay => &self.xray,
            _ if sectioned => &self.section,
            _ => &self.pipeline,
        };
        p.bind(cmd);
        p.push(cmd, &push);
        unsafe { d.cmd_draw(cmd, g.index_count, 1, 0, 0) };
        if mode == RenderMode::Edges
            && let Some((e, parts)) = &g.edges
        {
            self.edges.bind(cmd);
            self.edges.push(
                cmd,
                &EdgePush {
                    view_proj,
                    edges: e.address,
                    parts: parts.address,
                    params,
                    viewport: [w, h],
                    width: 1.4,
                    // Linéaire (0,012 ; 0,014 ; 0,018), opaque.
                    color: u32::from_le_bytes([3, 4, 5, 255]),
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
        let params = {
            let mut ring = self.params.borrow_mut();
            let bytes = ring.encode(camera);
            if ring.pick.size < bytes.len() as u64 {
                ring.pick = ParamRing::slot(&self.gpu, bytes.len())?;
            }
            ring.pick.mapped_mut().expect("paramètres non projetés")[..bytes.len()].copy_from_slice(&bytes);
            ring.pick.address
        };
        let t = &self.pick_target;
        let d = self.gpu.device().clone();
        let push = Push {
            view_proj,
            positions: g.positions.address,
            normals: g.normals.address,
            indices: g.indices.address,
            attrs: g.attrs.address,
            params,
            mode: 0,
            pad: 0,
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
        if hit == 0 || (depth <= 0.0 && !camera.ortho) {
            return Ok(None);
        }
        let k = g.ranges.partition_point(|(first, _)| *first <= triangle).saturating_sub(1);
        let (first, id) = g.ranges[k];
        // Point du pixel à la profondeur lue, par la matrice inverse (perspective comme orthographique).
        let Some(mut position) = camera.unproject(aspect, [cx, cy, depth]) else { return Ok(None) };
        // Bouchon de coupe : le point vu est sur le plan, le long du rayon du pixel.
        if hit == 2
            && let Some(plane) = self.params.borrow().section
            && let Some(near) = camera.unproject(aspect, [cx, cy, 1.0])
        {
            let n = [plane[0] as f64, plane[1] as f64, plane[2] as f64];
            let r = [position[0] - near[0], position[1] - near[1], position[2] - near[2]];
            let den = dot(n, r);
            if den.abs() > 1e-300 {
                let s = (plane[3] as f64 - dot(n, position)) / den;
                position = [position[0] + r[0] * s, position[1] + r[1] * s, position[2] + r[2] * s];
            }
        }
        Ok(Some(Pick { id, triangle: triangle - first, position }))
    }
}

impl ParamRing {
    fn slot(gpu: &Gpu, bytes: usize) -> Result<Buffer> {
        Buffer::new(
            gpu,
            "paramètres de l'image",
            bytes.max(64) as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            MemoryLocation::CpuToGpu,
        )
    }

    fn new(gpu: &Gpu, parts: usize) -> Result<Self> {
        let size = std::mem::size_of::<ParamsHeader>() + 16 * parts.max(1);
        let slots = (0..PARAM_SLOTS).map(|_| Self::slot(gpu, size)).collect::<Result<Vec<_>>>()?;
        let pick = Self::slot(gpu, size)?;
        let current = slots[0].address;
        Ok(ParamRing { slots, pick, next: 0, current, section: None, offsets: Vec::new() })
    }

    fn encode(&self, camera: &Camera) -> Vec<u8> {
        let eye = camera.eye();
        let f = camera.forward();
        let header = ParamsHeader {
            eye: [eye[0] as f32, eye[1] as f32, eye[2] as f32],
            ortho: camera.ortho as u32,
            forward: [f[0] as f32, f[1] as f32, f[2] as f32],
            flags: self.section.is_some() as u32,
            plane: self.section.unwrap_or([0.0, 0.0, 0.0, 0.0]),
        };
        let mut bytes = bytemuck::bytes_of(&header).to_vec();
        if self.offsets.is_empty() {
            bytes.extend_from_slice(&[0u8; 16]);
        }
        bytes.extend_from_slice(bytemuck::cast_slice(&self.offsets));
        bytes
    }
}
