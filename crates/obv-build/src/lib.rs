//! Compilation des shaders GLSL en SPIR-V, appelée depuis les `build.rs` du dépôt (D-05).
//!
//! Chaque fichier `*.vert`, `*.frag`, `*.comp`, `*.task`, `*.mesh` du dossier donné devient
//! `$OUT_DIR/<nom>.spv`. Les fichiers `*.glsl` sont des inclusions communes.

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

const STAGES: &[&str] = &["vert", "frag", "comp", "task", "mesh"];

/// Cherche `glslangValidator` : variable `GLSLANG`, puis `$VULKAN_SDK/bin`, puis le `PATH`.
fn glslang() -> PathBuf {
    if let Ok(p) = env::var("GLSLANG") {
        return PathBuf::from(p);
    }
    if let Ok(sdk) = env::var("VULKAN_SDK") {
        for name in ["glslangValidator.exe", "glslangValidator"] {
            let p = Path::new(&sdk).join("bin").join(name);
            if p.exists() {
                return p;
            }
        }
    }
    PathBuf::from("glslangValidator")
}

/// Compile tous les shaders de `dir` (relatif au crate appelant).
pub fn compile_shaders(dir: &str) {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let src = manifest.join(dir);
    println!("cargo:rerun-if-changed={}", src.display());
    println!("cargo:rerun-if-env-changed=GLSLANG");
    println!("cargo:rerun-if-env-changed=VULKAN_SDK");
    let tool = glslang();
    let mut entries: Vec<_> = std::fs::read_dir(&src)
        .unwrap_or_else(|e| panic!("dossier de shaders {}: {e}", src.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for path in entries {
        println!("cargo:rerun-if-changed={}", path.display());
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else { continue };
        if !STAGES.contains(&ext) {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy();
        let dst = out.join(format!("{name}.spv"));
        let output = Command::new(&tool)
            .args(["-V", "--target-env", "vulkan1.3", "-g0"])
            .arg(format!("-I{}", src.display()))
            .arg("-o")
            .arg(&dst)
            .arg(&path)
            .output()
            .unwrap_or_else(|e| {
                panic!(
                    "glslangValidator introuvable ({}): {e}. Installer glslang-tools (Linux) ou le SDK Vulkan (Windows).",
                    tool.display()
                )
            });
        if !output.status.success() {
            panic!(
                "échec de compilation de {name}:\n{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
