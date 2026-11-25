//! Vignettes des fichiers (D-41) : image 512 × 320 du modèle cadré, fond transparent (la carte peint le dégradé
//! de la vue dans le thème courant), rendue à l'ouverture et gardée sur disque pour l'accueil.

use std::{
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

use obv_ui::photos::{PHOTO_H, PHOTO_W};

fn dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("Obview 3D").join("vignettes"))
}

/// Fichier de la vignette d'un fichier (chemin canonique).
fn file(key: &Path) -> Option<PathBuf> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    dir().map(|d| d.join(format!("{:016x}.png", h.finish())))
}

/// Enregistre la vignette (fil séparé : l'encodage PNG ne retarde pas l'image).
pub fn save(key: &Path, rgba: Vec<u8>) {
    let Some(f) = file(key) else { return };
    std::thread::spawn(move || {
        let r = (|| -> Result<(), Box<dyn std::error::Error>> {
            std::fs::create_dir_all(f.parent().ok_or("dossier")?)?;
            let tmp = f.with_extension("png.tmp");
            let mut e = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(&tmp)?), PHOTO_W, PHOTO_H);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            e.write_header()?.write_image_data(&rgba)?;
            std::fs::rename(tmp, f)?;
            Ok(())
        })();
        if let Err(e) = r {
            eprintln!("Obview 3D : vignette non enregistrée : {e}");
        }
    });
}

/// Vignette lue sur le disque : fichier (chemin canonique), pixels RGBA.
pub type Stored = (PathBuf, Vec<u8>);

/// Vignettes enregistrées des fichiers `keys` (celles qui manquent ou sont illisibles sont omises).
pub fn load(keys: &[PathBuf]) -> Vec<Stored> {
    keys.iter().filter_map(|k| Some((k.clone(), read(&file(k)?)?))).collect()
}

fn read(f: &Path) -> Option<Vec<u8>> {
    let d = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(f).ok()?));
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    let ok = info.width == PHOTO_W
        && info.height == PHOTO_H
        && info.color_type == png::ColorType::Rgba
        && info.bit_depth == png::BitDepth::Eight;
    ok.then(|| {
        buf.truncate(info.buffer_size());
        buf
    })
}
