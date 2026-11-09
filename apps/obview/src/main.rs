//! Obview 3D : visionneuse 3D. Pour l'instant, `obview --info` affiche l'appareil Vulkan retenu.

use obv_gpu::{Gpu, GpuDesc};

fn main() {
    let validation = std::env::args().any(|a| a == "--validation");
    match Gpu::new(&GpuDesc { validation, ..Default::default() }) {
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
