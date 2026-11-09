//! Obview 3D : visionneuse 3D.
//!
//! - `obview [fichier]` ouvre la fenêtre (avec `--validation` : couche de validation Vulkan) ;
//! - `obview --info` affiche l'appareil Vulkan retenu.

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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--info") {
        info();
        return;
    }
    let path = args.iter().find(|a| !a.starts_with("--")).map(std::path::PathBuf::from);
    if let Err(e) = obview::app::run(path) {
        eprintln!("Obview 3D : {e}");
        std::process::exit(1);
    }
}
