//! Obview 3D : visionneuse 3D.
//!
//! - `obview [fichier]` ouvre la fenêtre (avec `--validation` : couche de validation Vulkan) ;
//! - `obview --info` affiche l'appareil Vulkan retenu ;
//! - `obview --convertir <entrée> <sortie.glb|obj|fbx> [--ecart <mm>]` convertit sans fenêtre.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use obv_gpu::{Gpu, GpuDesc};

fn info() {
    match Gpu::new(&GpuDesc::default()) {
        Ok(gpu) => {
            let c = gpu.caps();
            println!("Obview 3D {}", env!("CARGO_PKG_VERSION"));
            println!("Appareil      : {}", c.device_name);
            println!("Pilote        : {}", c.driver);
            println!(
                "Vulkan        : {}.{}.{}",
                obv_gpu::vk::api_version_major(c.api_version),
                obv_gpu::vk::api_version_minor(c.api_version),
                obv_gpu::vk::api_version_patch(c.api_version)
            );
            println!("Mesh shaders  : {}", if c.mesh_shader { "oui" } else { "non (chaîne classique)" });
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
        .filter(|(i, a)| !a.starts_with("--") && (*i == 0 || args[i - 1] != "--ecart"))
        .map(|(_, a)| a)
        .collect();
    let [input, output] = files[..] else { return Err("usage : --convertir <entrée> <sortie> [--ecart <mm>]".into()) };
    let tol_mm: f64 = match args.iter().position(|a| a == "--ecart") {
        Some(i) => args.get(i + 1).and_then(|v| v.replace(',', ".").parse().ok()).ok_or("écart illisible")?,
        None => 0.0,
    };
    let out = std::path::Path::new(output);
    let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let format = obv_mesh::Format::ALL
        .into_iter()
        .find(|f| f.extension() == ext)
        .ok_or_else(|| format!("format de sortie inconnu : .{ext}"))?;
    let t = std::time::Instant::now();
    let model = obv_io::read(std::path::Path::new(input)).map_err(|e| e.to_string())?;
    let t_read = t.elapsed();
    let opts = obv_mesh::ConvertOptions { format, tolerance_m: tol_mm / 1000.0 };
    let r = obv_mesh::convert(&model, out, opts, &|_| {}).map_err(|e| e.to_string())?;
    println!(
        "{} → {} triangles ({:.1} %), écart mesuré {:.4} mm, {:.1} Mo ; lecture {:.2} s, conversion {:.2} s",
        r.triangles_in(),
        r.triangles_out(),
        r.triangles_out() as f64 * 100.0 / r.triangles_in().max(1) as f64,
        r.deviation() as f64 * model.unit_m * 1000.0,
        r.bytes as f64 / 1e6,
        t_read.as_secs_f64(),
        (t.elapsed() - t_read).as_secs_f64()
    );
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--info") {
        info();
        return;
    }
    if args.iter().any(|a| a == "--convertir") {
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
