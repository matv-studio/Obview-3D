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
    /// Point survolé dans le rectangle de `hover`, en fractions (centre par défaut).
    hover_at: (f32, f32),
    tooltip: Option<Id>,
    size: (u32, u32),
    scale: f32,
    /// Réglage de l'état avant la première image.
    setup: fn(&mut AppState),
    /// Éléments cliqués, dans l'ordre, avant la capture.
    clicks: &'a [Id],
    /// Instant de la capture après le dernier clic (s) : par défaut, transitions terminées ; plus court, image
    /// prise en pleine transition.
    settle: f64,
}

impl Default for Shot<'_> {
    fn default() -> Self {
        Shot {
            name: "",
            theme: ThemeKind::Dark,
            select: None,
            hover: None,
            hover_at: (0.5, 0.5),
            tooltip: None,
            size: (1920, 1080),
            scale: 1.0,
            setup: |_| {},
            clicks: &[],
            settle: 2.5,
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
    let (mut ui, state, accent) = prepare(model, file_name, shot);
    let mut f = OffscreenFrame::new(gpu, w, h).unwrap();
    f.set_model(model, &state, accent).unwrap();
    f.render(&mut ui, &state).unwrap()
}

/// Même image par le moteur OpenGL (sans fenêtre, EGL) ; `None` si OpenGL 4.3 manque.
#[cfg(target_os = "linux")]
fn capture_gl(model: &Model, file_name: &str, shot: &Shot<'_>) -> Option<Vec<u8>> {
    let (w, h) = shot.size;
    let mut f = match obview::frame::GlOffscreenFrame::new(w, h) {
        Ok(f) => f,
        Err(e) => {
            assert!(std::env::var_os("OBV_REQUIRE_GL").is_none(), "OpenGL sans fenêtre exigé (OBV_REQUIRE_GL) : {e}");
            eprintln!("OpenGL sans fenêtre indisponible : {e}");
            return None;
        }
    };
    let (mut ui, state, accent) = prepare(model, file_name, shot);
    f.set_model(model, &state, accent).unwrap();
    Some(f.render(&mut ui, &state))
}

/// Interface et état après les clics et survols du cliché ; couleur de sélection en sRGB 8 bits.
fn prepare(model: &Model, file_name: &str, shot: &Shot<'_>) -> (Ui, AppState, [u8; 4]) {
    let (w, h) = shot.size;
    let logical = vec2(w as f32 / shot.scale, h as f32 / shot.scale);
    let mut state = AppState::new(file_name, model);
    state.settings.theme = if shot.theme == ThemeKind::Dark { ThemeChoice::Dark } else { ThemeChoice::Light };
    state.selected = shot.select;
    // Texte fixe : les références ne dépendent pas de la version de Mesa.
    state.renderer = (obview::gfx::Backend::Vulkan, "Vulkan 1.4 · llvmpipe".into());
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
    if shot.settle < 0.5 {
        // Image en pleine transition : aucune image intermédiaire.
        frame(&mut ui, &mut state, t - 0.1 + shot.settle, None, false);
    } else {
        frame(&mut ui, &mut state, t, None, false);
        let (fx, fy) = shot.hover_at;
        let pointer =
            shot.hover.and_then(|id| ui.rect_of(id)).map(|r| vec2(r.min.x + r.width() * fx, r.min.y + r.height() * fy));
        frame(&mut ui, &mut state, t + 0.5, pointer, false);
        ui.force_tooltip = shot.tooltip;
        // Animations terminées (apparition des fenêtres, interrupteurs).
        frame(&mut ui, &mut state, t + 1.5, pointer, false);
        frame(&mut ui, &mut state, t + 2.5, pointer, false);
    }
    let a = shell::selection_color(&ui.theme);
    let to8 =
        |c: f32| (if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 } * 255.0).round() as u8;
    (ui, state, [to8(a.r), to8(a.g), to8(a.b), 255])
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
    let base = Shot { select: Some(1), hover: Some(Id::new("gizmo")), ..Default::default() };
    shoot(&gpu, &model, &both(base, "coque_sombre", "coque_claire"));
    // Poignée du panneau de gauche survolée, avec son info-bulle.
    let grip = Shot { hover: Some(shell::grip_id(0)), tooltip: Some(shell::grip_id(0)), ..Default::default() };
    shoot(&gpu, &model, &both(grip, "coque_poignee_sombre", "coque_poignee_claire"));
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

/// Gizmos de navigation (D-25) : cube survolé, trièdre.
#[test]
fn gizmos_deux_themes() {
    use obview::settings::GizmoStyle;
    let gpu = Gpu::new(&GpuDesc { validation: true, ..Default::default() }).unwrap();
    let model = demo::bracket();
    let cube = Shot {
        hover: Some(Id::new("gizmo")),
        hover_at: (0.5, 0.42),
        setup: |s| s.settings.gizmo = GizmoStyle::Cube,
        ..Default::default()
    };
    shoot(&gpu, &model, &both(cube, "gizmo_cube_sombre", "gizmo_cube_clair"));
    let triad = Shot { setup: |s| s.settings.gizmo = GizmoStyle::Triad, ..Default::default() };
    shoot(&gpu, &model, &both(triad, "gizmo_triedre_sombre", "gizmo_triedre_clair"));
    assert_eq!(gpu.validation_errors(), 0, "erreurs de validation Vulkan");
}

/// Réglages : affichage (choix du gizmo) et boutons de la barre du bas (capture retirée).
#[test]
fn reglages_barre_et_affichage() {
    let gpu = Gpu::new(&GpuDesc { validation: true, ..Default::default() }).unwrap();
    let model = demo::bracket();
    let bar = Shot {
        setup: |s| {
            s.settings_open = true;
            s.settings_section = 3;
            s.settings.toolbar_hidden = vec![obview::settings::ToolButton::Capture];
        },
        ..Default::default()
    };
    shoot(&gpu, &model, &both(bar, "reglages_barre_sombre", "reglages_barre_claire"));
    let display = Shot {
        setup: |s| {
            s.settings_open = true;
            s.settings_section = 2;
        },
        hover: Some(Id::new("réglages").with("gizmo").with(1)),
        tooltip: Some(Id::new("réglages").with("gizmo").with(1)),
        ..Default::default()
    };
    shoot(&gpu, &model, &both(display, "reglages_affichage_sombre", "reglages_affichage_claire"));
    assert_eq!(gpu.validation_errors(), 0, "erreurs de validation Vulkan");
}

/// Version fictive proposée par les captures des mises à jour.
fn release() -> obview::update::Release {
    use obview::update::{Asset, Release, Version};
    Release {
        version: Version(0, 3, 0),
        page: String::new(),
        asset: Some(Asset {
            name: "Obview-3D-0.3.0-windows-x64-setup.exe".into(),
            url: String::new(),
            size: 1,
            sha256: None,
        }),
    }
}

/// Mises à jour (D-32) : réglages avec une version proposée (bouton survolé), barre du haut pendant le
/// téléchargement, après un échec.
#[test]
fn mises_a_jour_deux_themes() {
    use obview::update::Status;
    let gpu = Gpu::new(&GpuDesc { validation: true, ..Default::default() }).unwrap();
    let model = demo::bracket();
    let settings = Shot {
        setup: |s| {
            s.settings_open = true;
            s.settings_section = 4;
            s.app_version = "0.2.1";
            s.update = Status::Available(release());
        },
        hover: Some(Id::new("réglages").with("maj installer")),
        tooltip: Some(Id::new("réglages").with("maj installer")),
        ..Default::default()
    };
    shoot(&gpu, &model, &both(settings, "maj_reglages_sombre", "maj_reglages_claire"));
    let available = Shot {
        setup: |s| s.update = Status::Available(release()),
        hover: Some(Id::new("mise à jour")),
        tooltip: Some(Id::new("mise à jour")),
        ..Default::default()
    };
    shoot(&gpu, &model, &both(available, "maj_barre_sombre", "maj_barre_claire"));
    let downloading = Shot { setup: |s| s.update = Status::Downloading(release(), 0.42), ..Default::default() };
    shoot(&gpu, &model, &both(downloading, "maj_telechargement_sombre", "maj_telechargement_claire"));
    let current = Shot {
        setup: |s| {
            s.settings_open = true;
            s.settings_section = 4;
            s.app_version = "0.2.1";
            s.update = Status::Current;
        },
        ..Default::default()
    };
    shoot(&gpu, &model, &both(current, "maj_a_jour_sombre", "maj_a_jour_claire"));
    assert_eq!(gpu.validation_errors(), 0, "erreurs de validation Vulkan");
}

/// Points de mesure sur l'arête supérieure avant de la platine (pièce 0) et vers le moyeu.
fn measures(s: &mut AppState) {
    use obview::tools::MeasurePoint;
    let p = &s.parts[0];
    let (c, h) = (p.center, p.size.map(|v| v * 0.5));
    let a = MeasurePoint { part: 0, local: [(c[0] - h[0]) as f64, (c[1] + h[1]) as f64, (c[2] + h[2]) as f64] };
    let b = MeasurePoint { part: 0, local: [(c[0] + h[0]) as f64, (c[1] + h[1]) as f64, (c[2] + h[2]) as f64] };
    let m = &s.parts[1];
    let top = MeasurePoint {
        part: 1,
        local: [m.center[0] as f64, (m.center[1] + m.size[1] * 0.5) as f64, m.center[2] as f64],
    };
    s.measure.done = vec![[a, b]];
    s.measure.pending = Some(b);
    s.measure.hover = Some((top, obview::tools::Snap::Vertex));
    s.tool = obview::shell::Tool::Measure;
}

/// Outils de la vue (D-26) : mesure, coupe (poignée survolée), vue éclatée, vue orthographique.
#[test]
fn outils_deux_themes() {
    use obview::shell::Tool;
    let gpu = Gpu::new(&GpuDesc { validation: true, ..Default::default() }).unwrap();
    let model = demo::bracket();
    let mesure = Shot { setup: measures, ..Default::default() };
    shoot(&gpu, &model, &both(mesure, "outil_mesure_sombre", "outil_mesure_clair"));
    let coupe = Shot {
        setup: |s| {
            s.tool = Tool::Section;
            s.section.axis = 2;
            s.section.position = 0.45;
        },
        hover: Some(Id::new("outils").with("poignée coupe")),
        ..Default::default()
    };
    shoot(&gpu, &model, &both(coupe, "outil_coupe_sombre", "outil_coupe_clair"));
    let eclatee = Shot {
        setup: |s| {
            s.tool = Tool::Explode;
            s.explode = 0.7;
        },
        ..Default::default()
    };
    shoot(&gpu, &model, &both(eclatee, "outil_eclatee_sombre", "outil_eclatee_clair"));
    let ortho = Shot { setup: |s| s.camera.ortho = true, ..Default::default() };
    shoot(&gpu, &model, &both(ortho, "vue_orthographique_sombre", "vue_orthographique_claire"));
    assert_eq!(gpu.validation_errors(), 0, "erreurs de validation Vulkan");
}

/// Onglet « Vues » et fenêtre de capture.
#[test]
fn vues_et_capture() {
    use obview::views::SavedView;
    let gpu = Gpu::new(&GpuDesc { validation: true, ..Default::default() }).unwrap();
    let model = demo::bracket();
    let vues = Shot {
        setup: |s| {
            s.left_open = true;
            s.left_tab = 1;
            s.views.list = vec![
                SavedView::from_camera("Vue de détail".into(), &s.camera),
                SavedView::from_camera("Présentation".into(), &s.camera),
            ];
        },
        hover: Some(Id::new("vues").with("vue").with(0usize)),
        ..Default::default()
    };
    shoot(&gpu, &model, &both(vues, "onglet_vues_sombre", "onglet_vues_clair"));
    static CLICKS: std::sync::LazyLock<[Id; 1]> = std::sync::LazyLock::new(|| [Id::new("capture")]);
    let capture = Shot { clicks: &*CLICKS, ..Default::default() };
    shoot(&gpu, &model, &both(capture, "capture_fenetre_sombre", "capture_fenetre_claire"));
    assert_eq!(gpu.validation_errors(), 0, "erreurs de validation Vulkan");
}

/// Image de la vue sans interface, fond transparent : alpha nul dans un coin, plein sur la pièce.
#[test]
fn capture_transparente() {
    let gpu = Gpu::new(&GpuDesc { validation: true, ..Default::default() }).unwrap();
    let model = demo::bracket();
    let mut state = AppState::new("support.step", &model);
    state.viewport.set(obv_ui::Rect::from_xywh(0.0, 0.0, 640.0, 360.0));
    if let Some((lo, hi)) = model.bounds() {
        state.camera.frame(lo, hi, 640.0 / 360.0);
    }
    let theme = shell::theme(&state);
    let rgba = obview::frame::capture(&gpu, &model, &state, &theme, [80, 140, 255, 255], 640, 360, true).unwrap();
    assert_eq!(rgba.len(), 640 * 360 * 4);
    assert_eq!(rgba[3], 0, "coin transparent");
    let center = (180 * 640 + 320) * 4;
    assert_eq!(rgba[center + 3], 255, "pièce opaque");
    save_png(&out_dir().join("capture_transparente.png"), 640, 360, &rgba);
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

/// Ouverture (D-29) : vue vide (zone survolée), lecture en cours, avis d'échec.
#[test]
fn ouverture_deux_themes() {
    use obview::loading::{Loading, Notice};
    let gpu = Gpu::new(&GpuDesc { validation: true, ..Default::default() }).unwrap();
    let empty = Model::default();
    let vide = Shot { hover: Some(obview::loading::empty_id()), ..Default::default() };
    for shot in both(vide, "vide_sombre", "vide_clair") {
        let img = capture(&gpu, &empty, "", &shot);
        check_reference(shot.name, shot.size.0, shot.size.1, &img);
    }
    let lecture = Shot {
        setup: |s| {
            s.loading = Some(Loading {
                name: "WSSA0089.stp".into(),
                stage: obv_io::Stage::Transfer,
                fraction: Some(0.42),
                elapsed: 7.4,
                serial: 0,
            })
        },
        ..Default::default()
    };
    for shot in both(lecture, "lecture_sombre", "lecture_claire") {
        let img = capture(&gpu, &empty, "", &shot);
        check_reference(shot.name, shot.size.0, shot.size.1, &img);
    }
    let avis = Shot {
        setup: |s| {
            s.notice = Some(Notice {
                title: "Impossible d'ouvrir",
                subject: "piece.stl".into(),
                detail: "fichier invalide : STL trop court".into(),
            })
        },
        ..Default::default()
    };
    shoot(&gpu, &demo::bracket(), &both(avis, "avis_sombre", "avis_clair"));
    assert_eq!(gpu.validation_errors(), 0, "erreurs de validation Vulkan");
}

/// Interface en français (réglage Langue) : panneaux et réglages d'apparence.
#[test]
fn francais() {
    let gpu = Gpu::new(&GpuDesc { validation: true, ..Default::default() }).unwrap();
    let model = demo::bracket();
    let panels = Shot {
        name: "francais_panneaux",
        select: Some(1),
        setup: |s| {
            s.settings.language = obview::settings::Language::French;
            s.left_open = true;
            s.right_open = true;
        },
        ..Default::default()
    };
    let settings = Shot {
        name: "francais_reglages",
        setup: |s| {
            s.settings.language = obview::settings::Language::French;
            s.settings_open = true;
        },
        ..Default::default()
    };
    shoot(&gpu, &model, &[panels, settings]);
    assert_eq!(gpu.validation_errors(), 0, "erreurs de validation Vulkan");
}

/// Transitions prises en cours (D-29) : bandes des vues de rendu qui entrent, panneau qui glisse.
#[test]
fn transitions() {
    let gpu = Gpu::new(&GpuDesc { validation: true, ..Default::default() }).unwrap();
    let model = demo::bracket();
    static RENDER: std::sync::LazyLock<[Id; 1]> = std::sync::LazyLock::new(|| [Id::new("rendu")]);
    static GRIP: std::sync::LazyLock<[Id; 1]> = std::sync::LazyLock::new(|| [shell::grip_id(1)]);
    let bands = Shot { name: "transition_rendu", clicks: &*RENDER, settle: 0.17, ..Default::default() };
    let panel = Shot { name: "transition_panneau", select: Some(1), clicks: &*GRIP, settle: 0.1, ..Default::default() };
    shoot(&gpu, &model, &[bands, panel]);
    assert_eq!(gpu.validation_errors(), 0, "erreurs de validation Vulkan");
}

/// Moteur OpenGL de secours (D-30) : chaque écran rendu par OpenGL (llvmpipe) et par Vulkan (lavapipe) donne la
/// même image, à l'arrondi près (écart moyen sous 0,5 niveau, 99,9 % des pixels à 8 niveaux au plus).
#[test]
#[cfg(target_os = "linux")]
fn opengl_comme_vulkan() {
    let gpu = Gpu::new(&GpuDesc { validation: true, ..Default::default() }).unwrap();
    let model = demo::bracket();
    static RENDER: std::sync::LazyLock<[Id; 1]> = std::sync::LazyLock::new(|| [Id::new("rendu")]);
    let shots = [
        Shot {
            name: "gl_coque",
            select: Some(1),
            setup: |s| {
                s.left_open = true;
                s.right_open = true;
            },
            ..Default::default()
        },
        Shot { name: "gl_bandes", theme: ThemeKind::Light, clicks: &*RENDER, ..Default::default() },
        Shot {
            name: "gl_coupe",
            setup: |s| {
                s.tool = shell::Tool::Section;
            },
            ..Default::default()
        },
        Shot {
            name: "gl_aretes",
            setup: |s| {
                s.inspector = obview::inspector::Inspector::Single(obv_render::RenderMode::Edges);
            },
            ..Default::default()
        },
        Shot {
            name: "gl_rayons_x",
            setup: |s| {
                s.inspector = obview::inspector::Inspector::Single(obv_render::RenderMode::XRay);
            },
            ..Default::default()
        },
        Shot {
            name: "gl_ortho_eclatee",
            setup: |s| {
                s.camera.ortho = true;
                s.explode = 0.6;
            },
            ..Default::default()
        },
        Shot { name: "gl_reglages", setup: |s| s.settings_open = true, ..Default::default() },
    ];
    for shot in &shots {
        let vk = capture(&gpu, &model, "support.step", shot);
        let Some(gl) = capture_gl(&model, "support.step", shot) else { return };
        save_png(&out_dir().join(format!("{}_vulkan.png", shot.name)), shot.size.0, shot.size.1, &vk);
        save_png(&out_dir().join(format!("{}_opengl.png", shot.name)), shot.size.0, shot.size.1, &gl);
        let diffs: Vec<u8> = vk.iter().zip(&gl).map(|(a, b)| a.abs_diff(*b)).collect();
        let mean = diffs.iter().map(|d| *d as f64).sum::<f64>() / diffs.len() as f64;
        let mut px: Vec<u8> = diffs.chunks(4).map(|c| *c.iter().max().unwrap()).collect();
        px.sort_unstable();
        let p999 = px[px.len() * 999 / 1000];
        eprintln!("{} : écart moyen {mean:.3}, 99,9 % ≤ {p999}", shot.name);
        assert!(mean < 0.5 && p999 <= 8, "{} : OpenGL diffère de Vulkan (moyen {mean:.3}, 99,9 % {p999})", shot.name);
    }
    assert_eq!(gpu.validation_errors(), 0, "erreurs de validation Vulkan");
}

/// Captures sur un fichier local (`OBV_SAMPLE`, tout format lisible), à l'échelle 2, sans référence : `cargo test -- --ignored`.
#[test]
#[ignore]
fn captures_fichier_local() {
    let Some(path) = std::env::var_os("OBV_SAMPLE").or_else(|| std::env::var_os("OBV_SAMPLE_GLB")) else { return };
    let path = PathBuf::from(path);
    let t = std::time::Instant::now();
    let model = obview::app::load(&path).unwrap();
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
