//! Obview 3D : visionneuse 3D.
//!
//! - `obview [fichier]` ouvre la fenêtre (avec `--validation` : couche de validation Vulkan) ;
//! - `obview --info` affiche l'appareil Vulkan retenu ;
//! - `obview --convert <entrée> <sortie.glb|obj|fbx> [--tolerance <mm>] [--no-silhouette] [--no-compression | --meshopt]`
//!   convertit sans fenêtre (noms français acceptés : `--convertir`, `--ecart`, `--sans-silhouette`,
//!   `--sans-compression`). Messages dans la langue des réglages (anglais par défaut).

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use obv_gpu::{Gpu, GpuDesc};
use obv_ui::i18n::pick;

/// Option présente sous l'un de ses noms.
fn flag(args: &[String], names: &[&str]) -> bool {
    args.iter().any(|a| names.contains(&a.as_str()))
}

const TOLERANCE: &[&str] = &["--tolerance", "--ecart"];

fn info() {
    match Gpu::new(&GpuDesc::default()) {
        Ok(gpu) => {
            let c = gpu.caps();
            println!("Obview 3D {}", env!("CARGO_PKG_VERSION"));
            println!("{} : {}", pick("Device       ", "Appareil     "), c.device_name);
            println!("{} : {}", pick("Driver       ", "Pilote       "), c.driver);
            println!(
                "Vulkan        : {}.{}.{}",
                obv_gpu::vk::api_version_major(c.api_version),
                obv_gpu::vk::api_version_minor(c.api_version),
                obv_gpu::vk::api_version_patch(c.api_version)
            );
            println!(
                "Mesh shaders  : {}",
                if c.mesh_shader {
                    pick("yes", "oui")
                } else {
                    pick("no (classic pipeline)", "non (chaîne classique)")
                }
            );
        }
        Err(e) => {
            eprintln!("Obview 3D : {e}");
            std::process::exit(1);
        }
    }
}

fn convert(args: &[String]) -> Result<(), String> {
    let files: Vec<&String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !a.starts_with("--") && (*i == 0 || !TOLERANCE.contains(&args[i - 1].as_str())))
        .map(|(_, a)| a)
        .collect();
    let [input, output] = files[..] else {
        return Err(pick(
            "usage: --convert <input> <output> [--tolerance <mm>]",
            "usage : --convertir <entrée> <sortie> [--ecart <mm>]",
        )
        .into());
    };
    let tol_mm: f64 = match args.iter().position(|a| TOLERANCE.contains(&a.as_str())) {
        Some(i) => args
            .get(i + 1)
            .and_then(|v| v.replace(',', ".").parse().ok())
            .ok_or(pick("unreadable tolerance", "écart illisible"))?,
        None => 0.0,
    };
    let out = std::path::Path::new(output);
    let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let format = obv_mesh::Format::ALL
        .into_iter()
        .find(|f| f.extension() == ext)
        .ok_or_else(|| format!("{} .{ext}", pick("unknown output format:", "format de sortie inconnu :")))?;
    let t = std::time::Instant::now();
    let model = obview::app::load(std::path::Path::new(input)).map_err(|e| obv_ui::i18n::tr_message(&e))?;
    let t_read = t.elapsed();
    let opts = obv_mesh::ConvertOptions {
        format,
        tolerance_m: tol_mm / 1000.0,
        keep_silhouette: !flag(args, &["--no-silhouette", "--sans-silhouette"]),
        compression: if flag(args, &["--no-compression", "--sans-compression"]) {
            obv_mesh::Compression::None
        } else if args.iter().any(|a| a == "--meshopt") {
            obv_mesh::Compression::Meshopt
        } else {
            obv_mesh::Compression::Quantize
        },
    };
    let r = obv_mesh::convert(&model, out, opts, &|_| {}).map_err(|e| obv_ui::i18n::tr_message(&e.to_string()))?;
    let (tin, tout) = (r.triangles_in(), r.triangles_out());
    let ratio = tout as f64 * 100.0 / tin.max(1) as f64;
    let dev = r.deviation() as f64 * model.unit_m * 1000.0;
    let mb = r.bytes as f64 / 1e6;
    let (read, conv) = (t_read.as_secs_f64(), (t.elapsed() - t_read).as_secs_f64());
    if obv_ui::i18n::is_fr() {
        println!(
            "{tin} → {tout} triangles ({ratio:.1} %), écart mesuré {dev:.4} mm, {mb:.1} Mo ; lecture {read:.2} s, conversion {conv:.2} s"
        );
    } else {
        println!(
            "{tin} → {tout} triangles ({ratio:.1}%), measured deviation {dev:.4} mm, {mb:.1} MB; read {read:.2} s, conversion {conv:.2} s"
        );
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    obview::lang::init();
    obv_ui::i18n::set_lang(obview::settings::Settings::load().language.lang());
    if args.iter().any(|a| a == "--info") {
        info();
        return;
    }
    if flag(&args, &["--convert", "--convertir"]) {
        if let Err(e) = convert(&args) {
            eprintln!("Obview 3D : {e}");
            std::process::exit(1);
        }
        return;
    }
    let path = args.iter().find(|a| !a.starts_with("--")).map(std::path::PathBuf::from);
    if let Err(e) = obview::app::run(path) {
        eprintln!("Obview 3D : {e}");
        std::process::exit(1);
    }
}
