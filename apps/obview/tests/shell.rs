//! Captures de la coque dans les deux thèmes (D-06), comparées à des références.

use std::path::{Path, PathBuf};

use obv_gpu::{Gpu, GpuDesc};
use obv_io::Model;
use obv_ui::{FrameInput, Id, ThemeKind, Ui, vec2};
use obview::{
    demo,
    frame::OffscreenFrame,
    settings::ThemeChoice,
    shell::{self, AppState},
};

#[derive(Clone, Copy)]
struct Shot<'a> {
    name: &'a str,
    theme: ThemeKind,
    select: Option<usize>,
    hover: Option<Id>,
    tooltip: Option<Id>,
    size: (u32, u32),
    scale: f32,
    /// Réglage de l'état avant la première image.
    setup: fn(&mut AppState),
    /// Éléments cliqués, dans l'ordre, avant la capture.
    clicks: &'a [Id],
}

impl Default for Shot<'_> {
    fn default() -> Self {
        Shot {
            name: "",
            theme: ThemeKind::Dark,
            select: None,
            hover: None,
            tooltip: None,
            size: (1920, 1080),
            scale: 1.0,
            setup: |_| {},
            clicks: &[],
        }
    }
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
    assert_eq!((rw, rh), (w, h));
    let bad = rgba
        .chunks(4)
        .zip(expected.chunks(4))
        .filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) > 3))
        .count();
    let ratio = bad as f64 / (w * h) as f64;
    assert!(ratio < 0.002, "{name} : {:.3} % des pixels diffèrent ({})", ratio * 100.0, actual.display());
}

/// Rend une capture : une image pour la mise en page, puis le survol éventuel, puis l'image finale.
fn capture(gpu: &Gpu, model: &Model, file_name: &str, shot: &Shot<'_>) -> Vec<u8> {
    let (w, h) = shot.size;
    let logical = vec2(w as f32 / shot.scale, h as f32 / shot.scale);
    let mut state = AppState::new(file_name, model);
    state.settings.theme = if shot.theme == ThemeKind::Dark { ThemeChoice::Dark } else { ThemeChoice::Light };
    state.selected = shot.select;
    (shot.setup)(&mut state);
    let mut ui = Ui::new(shell::theme(&state));
    let frame = |ui: &mut Ui, state: &mut AppState, time: f64, pointer, down: bool| {
        let buttons = [down, false, false];
        ui.begin_frame(FrameInput { size: logical, scale: shot.scale, time, pointer, buttons, ..Default::default() });
        shell::handle(ui, state);
        ui.theme = shell::theme(state);
        let mut root = shell::build(ui, state);
        ui.finish(&mut root);
    };
    frame(&mut ui, &mut state, 0.0, None, false);
    let v = state.viewport.get();
    if let Some((lo, hi)) = model.bounds() {
        state.camera.frame(lo, hi, (v.width() / v.height()) as f64);
    }
    let mut t = 0.1;
    for id in shot.clicks {
        frame(&mut ui, &mut state, t, None, false);
        let p = ui.rect_of(*id).map(|r| r.center());
        assert!(p.is_some(), "{} : élément à cliquer absent", shot.name);
        frame(&mut ui, &mut state, t + 0.05, p, true);
        frame(&mut ui, &mut state, t + 0.1, p, false);
        t += 0.2;
    }
    frame(&mut ui, &mut state, t, None, false);
    let pointer = shot.hover.and_then(|id| ui.rect_of(id)).map(|r| r.center());
    frame(&mut ui, &mut state, t + 0.5, pointer, false);
    ui.force_tooltip = shot.tooltip;
    // Animations terminées (apparition des fenêtres, interrupteurs).
    frame(&mut ui, &mut state, t + 1.5, pointer, false);
    frame(&mut ui, &mut state, t + 2.5, pointer, false);
    let mut f = OffscreenFrame::new(gpu, w, h).unwrap();
    let a = shell::selection_color(&ui.theme);
    let to8 =
        |c: f32| (if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 } * 255.0).round() as u8;
    f.set_model(model, &state, [to8(a.r), to8(a.g), to8(a.b), 255]).unwrap();
    f.render(&mut ui, &state).unwrap()
}

fn shoot(gpu: &Gpu, model: &Model, shots: &[Shot<'_>]) {
    for shot in shots {
        let img = capture(gpu, model, "support.step", shot);
        check_reference(shot.name, shot.size.0, shot.size.1, &img);
    }
}

fn both<'a>(base: Shot<'a>, dark: &'a str, light: &'a str) -> [Shot<'a>; 2] {
    [Shot { name: dark, theme: ThemeKind::Dark, ..base }, Shot { name: light, theme: ThemeKind::Light, ..base }]
}

/// Coque en 1920 × 1080 (D-16), dans les deux thèmes (D-06) : vue seule par défaut (gizmo survolé), puis
/// panneaux ouverts avec une sélection.
#[test]
fn coque_deux_themes() {
    let gpu = Gpu::new(&GpuDesc { validation: true, ..Default::default() }).unwrap();
    let model = demo::bracket();
    let base = Shot { select: Some(1), hover: Some(Id::new("cube de vue")), ..Default::default() };
    shoot(&gpu, &model, &both(base, "coque_sombre", "coque_claire"));
    let panels = Shot {
        select: Some(1),
        hover: Some(Id::new("pièce").with(3)),
        tooltip: Some(Id::new("outil").with(1u8)),
        setup: |s| {
            s.left_open = true;
            s.right_open = true;
        },
        ..Default::default()
    };
    shoot(&gpu, &model, &both(panels, "coque_panneaux_sombre", "coque_panneaux_claire"));
    assert_eq!(gpu.validation_errors(), 0, "erreurs de validation Vulkan");
}

/// Fenêtre des réglages : navigation (liste des schémas ouverte), apparence avec un accent personnalisé.
#[test]
fn reglages_deux_themes() {
    let gpu = Gpu::new(&GpuDesc { validation: true, ..Default::default() }).unwrap();
    let model = demo::bracket();
    let nav = Shot {
        setup: |s| {
            s.settings_open = true;
            s.settings_section = 1;
        },
        clicks: &[],
        ..Default::default()
    };
    shoot(&gpu, &model, &both(nav, "reglages_navigation_sombre", "reglages_navigation_claire"));
    let apparence = Shot {
        setup: |s| {
            s.settings_open = true;
            s.settings_section = 0;
            s.settings.accent = Some(0xe0559b);
        },
        hover: Some(Id::new("réglages").with("accent").with(3)),
        ..Default::default()
    };
    shoot(&gpu, &model, &both(apparence, "reglages_apparence_sombre", "reglages_apparence_claire"));
    assert_eq!(gpu.validation_errors(), 0, "erreurs de validation Vulkan");
}

/// Liste des schémas de navigation ouverte (menu au-dessus de la fenêtre modale).
#[test]
fn reglages_liste_ouverte() {
    let gpu = Gpu::new(&GpuDesc { validation: true, ..Default::default() }).unwrap();
    let model = demo::bracket();
    static CLICKS: std::sync::LazyLock<[Id; 1]> = std::sync::LazyLock::new(|| [Id::new("réglages").with("schéma")]);
    let shot = Shot {
        setup: |s| {
            s.settings_open = true;
            s.settings_section = 1;
        },
        clicks: &*CLICKS,
        hover: Some(Id::new("réglages").with("schéma").with(2)),
        ..Default::default()
    };
    shoot(&gpu, &model, &both(shot, "reglages_liste_sombre", "reglages_liste_claire"));
    assert_eq!(gpu.validation_errors(), 0, "erreurs de validation Vulkan");
}

/// Vues de rendu (D-15) : bandes, puis deux rendus isolés.
#[test]
fn inspecteur_deux_themes() {
    use obv_render::RenderMode;
    use obview::inspector::Inspector;
    let gpu = Gpu::new(&GpuDesc { validation: true, ..Default::default() }).unwrap();
    let model = demo::bracket();
    let bands = Shot { setup: |s| s.inspector = Inspector::Bands, ..Default::default() };
    shoot(&gpu, &model, &both(bands, "inspecteur_sombre", "inspecteur_clair"));
    let topo = Shot { setup: |s| s.inspector = Inspector::Single(RenderMode::Topology), ..Default::default() };
    let edges = Shot { setup: |s| s.inspector = Inspector::Single(RenderMode::Edges), ..Default::default() };
    shoot(&gpu, &model, &[Shot { name: "inspecteur_topologie", ..topo }, Shot { name: "inspecteur_aretes", ..edges }]);
    assert_eq!(gpu.validation_errors(), 0, "erreurs de validation Vulkan");
}

/// Fenêtre de conversion : réglages, puis bilan.
#[test]
fn conversion_deux_themes() {
    use obview::convert_ui::Status;
    let gpu = Gpu::new(&GpuDesc { validation: true, ..Default::default() }).unwrap();
    let model = demo::bracket();
    let idle = Shot {
        setup: |s| s.convert.open = true,
        hover: Some(Id::new("convertir").with("densité")),
        ..Default::default()
    };
    shoot(&gpu, &model, &both(idle, "conversion_sombre", "conversion_claire"));
    let done = Shot {
        setup: |s| {
            s.convert.open = true;
            s.convert.status = Status::Done {
                triangles_in: 996,
                triangles_out: 412,
                deviation_mm: 0.0187,
                bytes: 25_400,
                path: "support.glb".into(),
            };
        },
        ..Default::default()
    };
    shoot(&gpu, &model, &both(done, "conversion_bilan_sombre", "conversion_bilan_claire"));
    assert_eq!(gpu.validation_errors(), 0, "erreurs de validation Vulkan");
}

/// Captures sur un fichier local (`OBV_SAMPLE_GLB`), à l'échelle 2, sans référence : `cargo test -- --ignored`.
#[test]
#[ignore]
fn captures_fichier_local() {
    let Some(path) = std::env::var_os("OBV_SAMPLE_GLB") else { return };
    let path = PathBuf::from(path);
    let t = std::time::Instant::now();
    let model = obv_io::read_glb(&path).unwrap();
    eprintln!(
        "lecture : {:.0} ms, {} pièces, {} triangles",
        t.elapsed().as_secs_f64() * 1000.0,
        model.parts.len(),
        model.triangle_count()
    );
    let gpu = Gpu::new(&GpuDesc { validation: false, ..Default::default() }).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let select = std::env::var("OBV_SELECT").ok().and_then(|s| s.parse().ok());
    // Taille et échelle de la capture (`OBV_SIZE=2560x1440`, `OBV_SCALE=1`), panneaux ouverts (`OBV_PANELS=1`).
    let (w, h) = std::env::var("OBV_SIZE")
        .ok()
        .and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))))
        .unwrap_or((3840u32, 2160u32));
    let scale = std::env::var("OBV_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(2.0);
    let panels = std::env::var_os("OBV_PANELS").is_some();
    for (n, theme) in [("local_sombre", ThemeKind::Dark), ("local_clair", ThemeKind::Light)] {
        let mut shot = Shot { name: n, theme, select, size: (w, h), scale, ..Default::default() };
        if panels {
            shot.setup = |s| {
                s.left_open = true;
                s.right_open = true;
            };
        }
        let img = capture(&gpu, &model, &name, &shot);
        save_png(&out_dir().join(format!("{n}.png")), w, h, &img);
    }
}

#[test]
fn survol_ligne_arbre() {
    let model = demo::bracket();
    let mut state = AppState::new("support.step", &model);
    let mut ui = Ui::new(shell::theme(&state));
    let size = vec2(1920.0, 1080.0);
    let run = |ui: &mut Ui, state: &mut AppState, time: f64, pointer| {
        ui.begin_frame(FrameInput { size, scale: 1.0, time, pointer, ..Default::default() });
        shell::handle(ui, state);
        let mut root = shell::build(ui, state);
        ui.finish(&mut root);
    };
    state.left_open = true;
    run(&mut ui, &mut state, 0.0, None);
    let id = Id::new("pièce").with(3usize);
    let r = ui.rect_of(id).expect("ligne absente");
    run(&mut ui, &mut state, 0.5, Some(r.center()));
    assert!(ui.hovered(id), "la ligne n'est pas survolée");
}
