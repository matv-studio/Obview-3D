//! Icône et description de l'exécutable Windows.

fn main() {
    println!("cargo:rerun-if-changed=assets/obview.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut r = winresource::WindowsResource::new();
        r.set_icon("assets/obview.ico");
        r.set("ProductName", "Obview 3D");
        r.set("FileDescription", "Obview 3D");
        r.compile().expect("ressources Windows (icône)");
    }
}
