//! Compile le pont C++ et lie OpenCASCADE construit par `tools/occt.sh` (D-27).
//!
//! `OCCT_DIR` : dossier d'installation (include/opencascade, lib). Absent : lecture STEP / IGES désactivée,
//! sauf si `OBV_REQUIRE_OCCT` est défini (construction des versions), auquel cas la construction échoue.

use std::{env, path::PathBuf};

/// Boîtes à outils, des plus dépendantes aux plus fondamentales (ordre de l'éditeur de liens statique).
const TOOLKITS: &[&str] = &[
    "TKXCAF",
    "TKDESTEP",
    "TKDEIGES",
    "TKDE",
    "TKXSBase",
    "TKVCAF",
    "TKCAF",
    "TKLCAF",
    "TKCDF",
    "TKV3d",
    "TKService",
    "TKMesh",
    "TKHLR",
    "TKShHealing",
    "TKBool",
    "TKBO",
    "TKPrim",
    "TKTopAlgo",
    "TKGeomAlgo",
    "TKBRep",
    "TKGeomBase",
    "TKG3d",
    "TKG2d",
    "TKMath",
    "TKernel",
];

fn main() {
    println!("cargo::rustc-check-cfg=cfg(occt)");
    println!("cargo:rerun-if-env-changed=OCCT_DIR");
    println!("cargo:rerun-if-env-changed=OBV_REQUIRE_OCCT");
    println!("cargo:rerun-if-changed=cpp");
    println!("cargo:rerun-if-changed=src/ffi.rs");
    let Some(dir) = env::var_os("OCCT_DIR").map(PathBuf::from) else {
        if env::var_os("OBV_REQUIRE_OCCT").is_some() {
            panic!("OCCT_DIR absent : construire OpenCASCADE avec tools/occt.sh");
        }
        println!("cargo:warning=OCCT_DIR absent : lecture STEP / IGES désactivée dans cette construction");
        return;
    };
    let include = dir.join("include").join("opencascade");
    let lib = dir.join("lib");
    assert!(include.join("Standard.hxx").exists(), "OCCT_DIR sans include/opencascade : {}", dir.display());
    println!("cargo:rustc-cfg=occt");
    let mut b = cxx_build::bridge("src/ffi.rs");
    b.file("cpp/reader.cpp").include(&include).include("cpp").std("c++17").warnings(false);
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        b.flag("/EHsc").flag("/bigobj").flag("/utf-8");
    }
    b.compile("obv_cad_bridge");
    println!("cargo:rustc-link-search=native={}", lib.display());
    for tk in TOOLKITS {
        println!("cargo:rustc-link-lib=static={tk}");
    }
    match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("windows") => {
            for l in ["user32", "advapi32", "gdi32", "shell32", "ws2_32", "wsock32"] {
                println!("cargo:rustc-link-lib=dylib={l}");
            }
        }
        Ok("macos") => {
            println!("cargo:rustc-link-lib=dylib=c++");
            println!("cargo:rustc-link-lib=framework=Foundation");
            println!("cargo:rustc-link-lib=framework=AppKit");
        }
        _ => {
            for l in ["stdc++", "pthread", "dl", "m"] {
                println!("cargo:rustc-link-lib=dylib={l}");
            }
        }
    }
}
