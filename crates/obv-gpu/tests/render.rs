//! Essais de rendu hors écran (D-06) : la chaîne classique et les mesh shaders doivent donner la même image,
//! conforme à la référence, sans aucune erreur de validation.

use std::path::{Path, PathBuf};

use glam::{Mat4, Vec3};
use obv_gpu::{Buffer, Gpu, GpuDesc, GraphicsPipeline, PipelineDesc, RenderTarget, ShaderStages, spirv, vk};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Push {
    view_proj: [[f32; 4]; 4],
    positions: u64,
    normals: u64,
    indices: u64,
    index_count: u32,
    _pad: u32,
}

/// Cube à faces plates : 24 sommets, 36 indices, sens trigonométrique vu de l'extérieur.
fn cube() -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
    ];
    let (mut p, mut n, mut idx) = (Vec::new(), Vec::new(), Vec::new());
    for (normal, u, v) in faces {
        let (nv, uv, vv) = (Vec3::from(normal), Vec3::from(u), Vec3::from(v));
        let base = p.len() as u32;
        for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            p.push((nv + uv * a + vv * b).into());
            n.push(normal);
        }
        idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (p, n, idx)
}

/// Perspective à profondeur inversée et plan lointain infini (repère main droite, profondeur 0..1).
fn perspective_reverse_infinite(fov_y: f32, aspect: f32, near: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    Mat4::from_cols_array(&[f / aspect, 0.0, 0.0, 0.0, 0.0, f, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, near, 0.0])
}

fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    let f = (target - eye).normalize();
    let s = f.cross(up).normalize();
    let u = s.cross(f);
    Mat4::from_cols_array(&[
        s.x,
        u.x,
        -f.x,
        0.0,
        s.y,
        u.y,
        -f.y,
        0.0,
        s.z,
        u.z,
        -f.z,
        0.0,
        -s.dot(eye),
        -u.dot(eye),
        f.dot(eye),
        1.0,
    ])
}

fn out_dir() -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rendus");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn save_png(path: &Path, w: u32, h: u32, rgba: &[u8]) {
    let f = std::fs::File::create(path).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(rgba).unwrap();
}

fn load_png(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let d = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).ok()?));
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    Some((info.width, info.height, buf))
}

/// Part des pixels dont un canal diffère de plus de `tol`.
fn diff_ratio(a: &[u8], b: &[u8], tol: u8) -> f64 {
    let bad = a.chunks(4).zip(b.chunks(4)).filter(|(x, y)| x.iter().zip(*y).any(|(p, q)| p.abs_diff(*q) > tol)).count();
    bad as f64 / (a.len() / 4) as f64
}

/// Compare à `tests/reference/<nom>.png` ; `OBV_UPDATE_REFERENCE=1` réécrit la référence.
fn check_reference(name: &str, w: u32, h: u32, rgba: &[u8]) {
    let actual = out_dir().join(format!("{name}.png"));
    save_png(&actual, w, h, rgba);
    let reference = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/reference").join(format!("{name}.png"));
    if std::env::var_os("OBV_UPDATE_REFERENCE").is_some() {
        std::fs::create_dir_all(reference.parent().unwrap()).unwrap();
        std::fs::copy(&actual, &reference).unwrap();
        return;
    }
    let (rw, rh, expected) = load_png(&reference).unwrap_or_else(|| {
        panic!("référence absente : {} (OBV_UPDATE_REFERENCE=1 pour la créer)", reference.display())
    });
    assert_eq!((rw, rh), (w, h), "taille différente de la référence");
    let r = diff_ratio(rgba, &expected, 2);
    assert!(r < 0.001, "{name} : {:.3} % des pixels diffèrent de la référence ({})", r * 100.0, actual.display());
}

fn gpu() -> Gpu {
    Gpu::new(&GpuDesc { validation: true, ..Default::default() }).expect("Vulkan 1.3 requis (lavapipe en essai)")
}

#[test]
fn cube_classique_et_mesh_shaders() {
    let gpu = gpu();
    eprintln!("{:?}", gpu.caps());
    let (pos, nrm, idx) = cube();
    let usage = vk::BufferUsageFlags::STORAGE_BUFFER;
    let bp = Buffer::with_data(&gpu, "positions", bytemuck::cast_slice(&pos), usage).unwrap();
    let bn = Buffer::with_data(&gpu, "normales", bytemuck::cast_slice(&nrm), usage).unwrap();
    let bi = Buffer::with_data(&gpu, "indices", bytemuck::cast_slice(&idx), usage).unwrap();

    let (w, h) = (256, 192);
    let proj = perspective_reverse_infinite(40f32.to_radians(), w as f32 / h as f32, 0.1);
    let view = look_at(Vec3::new(3.2, 2.4, 4.0), Vec3::ZERO, Vec3::Y);
    let push = Push {
        view_proj: (proj * view).to_cols_array_2d(),
        positions: bp.address,
        normals: bn.address,
        indices: bi.address,
        index_count: idx.len() as u32,
        _pad: 0,
    };
    let target = RenderTarget::new(&gpu, w, h).unwrap();
    let clear = [0.012, 0.014, 0.018, 1.0];
    let base = PipelineDesc {
        color_formats: &[RenderTarget::COLOR_FORMAT],
        depth_format: Some(RenderTarget::DEPTH_FORMAT),
        push_constant_size: std::mem::size_of::<Push>() as u32,
        ..Default::default()
    };

    let classic = GraphicsPipeline::new(
        &gpu,
        &PipelineDesc {
            stages: ShaderStages::Vertex { vertex: spirv!("test_cube.vert"), fragment: spirv!("test_shade.frag") },
            ..base
        },
    )
    .unwrap();
    target
        .render(clear, |cmd| {
            classic.bind(cmd);
            classic.push(cmd, &push);
            unsafe { gpu.device().cmd_draw(cmd, idx.len() as u32, 1, 0, 0) };
        })
        .unwrap();
    let img_classic = target.read_rgba8();
    check_reference("cube", w, h, &img_classic);

    if gpu.caps().mesh_shader {
        let mesh = GraphicsPipeline::new(
            &gpu,
            &PipelineDesc {
                stages: ShaderStages::Mesh {
                    task: None,
                    mesh: spirv!("test_cube.mesh"),
                    fragment: spirv!("test_shade.frag"),
                },
                color_formats: &[RenderTarget::COLOR_FORMAT],
                depth_format: Some(RenderTarget::DEPTH_FORMAT),
                push_constant_size: std::mem::size_of::<Push>() as u32,
                ..Default::default()
            },
        )
        .unwrap();
        target
            .render(clear, |cmd| {
                mesh.bind(cmd);
                mesh.push(cmd, &push);
                unsafe { gpu.mesh_shader().unwrap().cmd_draw_mesh_tasks(cmd, 1, 1, 1) };
            })
            .unwrap();
        let img_mesh = target.read_rgba8();
        save_png(&out_dir().join("cube_mesh.png"), w, h, &img_mesh);
        let r = diff_ratio(&img_classic, &img_mesh, 1);
        assert!(r < 0.0005, "mesh shaders et chaîne classique diffèrent sur {:.3} % des pixels", r * 100.0);
    } else {
        eprintln!("mesh shaders absents sur {} : seul le rendu classique est vérifié", gpu.caps().device_name);
    }
    assert_eq!(gpu.validation_errors(), 0, "erreurs de validation Vulkan");
}
