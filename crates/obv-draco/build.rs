//! Compile le pont C++ et lie Draco construit par `tools/draco.sh` (D-30).
//!
//! `DRACO_DIR` : dossier d'installation (include/draco, lib). Absent : compression et lecture Draco désactivées,
//! sauf si `OBV_REQUIRE_DRACO` est défini (CI, versions), auquel cas la construction échoue.

use std::{env, path::PathBuf};

fn main() {
    println!("cargo::rustc-check-cfg=cfg(draco)");
    println!("cargo:rerun-if-env-changed=DRACO_DIR");
    println!("cargo:rerun-if-env-changed=OBV_REQUIRE_DRACO");
    println!("cargo:rerun-if-changed=cpp");
    println!("cargo:rerun-if-changed=src/ffi.rs");
    let Some(dir) = env::var_os("DRACO_DIR").map(PathBuf::from) else {
        if env::var_os("OBV_REQUIRE_DRACO").is_some() {
            panic!("DRACO_DIR absent : construire Draco avec tools/draco.sh");
        }
        println!("cargo:warning=DRACO_DIR absent : compression Draco désactivée dans cette construction");
        return;
    };
    let include = dir.join("include");
    assert!(include.join("draco/compression/encode.h").exists(), "DRACO_DIR sans include/draco : {}", dir.display());
    println!("cargo:rustc-cfg=draco");
    let mut b = cxx_build::bridge("src/ffi.rs");
    b.file("cpp/codec.cpp").include(&include).include("cpp").std("c++17").warnings(false);
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        b.flag("/EHsc").flag("/utf-8");
    }
    b.compile("obv_draco_bridge");
    println!("cargo:rustc-link-search=native={}", dir.join("lib").display());
    println!("cargo:rustc-link-lib=static=draco");
    match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("windows") => {}
        Ok("macos") => println!("cargo:rustc-link-lib=dylib=c++"),
        _ => println!("cargo:rustc-link-lib=dylib=stdc++"),
    }
}
