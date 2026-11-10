//! Compilation des shaders GLSL en SPIR-V, appelée depuis les `build.rs` du dépôt (D-05).
//!
//! Chaque fichier `*.vert`, `*.frag`, `*.comp`, `*.task`, `*.mesh` du dossier donné devient
//! `$OUT_DIR/<nom>.spv`. Les fichiers `*.glsl` sont des inclusions communes.
//!
//! Moteur OpenGL de secours (D-30) : les nuanceurs de sommets et de fragments sont aussi prétraités avec
//! `OBV_GL` défini (inclusions développées, tampons par liaison au lieu des adresses) en GLSL 4.30 :
//! `$OUT_DIR/<nom>.gl`, vérifié par glslangValidator à la construction et compilé par le pilote à l'exécution.

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

/// Compile tous les shaders de `dir` (relatif au crate appelant), pour Vulkan seulement.
pub fn compile_shaders(dir: &str) {
    compile(dir, false);
}

/// Comme [`compile_shaders`], avec en plus les variantes OpenGL des nuanceurs de sommets et de fragments.
pub fn compile_shaders_with_gl(dir: &str) {
    compile(dir, true);
}

fn compile(dir: &str, with_gl: bool) {
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
        if with_gl && matches!(ext, "vert" | "frag") {
            gl_variant(&tool, &src, &path, ext, &out.join(format!("{name}.gl")));
        }
    }
}

/// Variante OpenGL 4.30 d'un nuanceur : prétraitement avec `OBV_GL` (et `OBV_STAGE_FRAG` pour les fragments),
/// lignes propres à Vulkan retirées, puis vérification.
fn gl_variant(tool: &Path, src: &Path, path: &Path, ext: &str, dst: &Path) {
    let name = path.file_name().unwrap().to_string_lossy();
    let mut cmd = Command::new(tool);
    cmd.args(["-E", "-DOBV_GL"]).arg(format!("-I{}", src.display()));
    if ext == "frag" {
        cmd.arg("-DOBV_STAGE_FRAG");
    }
    let output = cmd.arg(path).output().expect("glslangValidator");
    if !output.status.success() {
        panic!("échec du prétraitement OpenGL de {name}:\n{}", String::from_utf8_lossy(&output.stdout));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut gl = String::from("#version 430 core\n");
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with("#version")
            || t.starts_with("#line")
            || t.starts_with("#extension GL_GOOGLE_include_directive")
        {
            continue;
        }
        if t.is_empty() {
            continue;
        }
        gl.push_str(line);
        gl.push('\n');
    }
    std::fs::write(dst, &gl).unwrap_or_else(|e| panic!("{}: {e}", dst.display()));
    // Vérification avec les définitions ajoutées à l'exécution (voir obv-gl).
    let probe = dst.with_extension("gl.check");
    std::fs::write(&probe, gl.replacen("#version 430 core\n", "#version 430 core\n#define OBV_DEPTH_REMAP 0.0\n", 1))
        .unwrap_or_else(|e| panic!("{}: {e}", probe.display()));
    let check = Command::new(tool).args(["-S", ext]).arg(&probe).output().expect("glslangValidator");
    if !check.status.success() {
        panic!("nuanceur OpenGL {name} invalide:\n{}\n--- source ---\n{gl}", String::from_utf8_lossy(&check.stdout));
    }
}
