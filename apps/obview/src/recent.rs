//! Fichiers récemment ouverts (D-33, D-41) : liste enregistrée à part des réglages (« Rétablir » ne la vide pas),
//! montrée par la page d'accueil. Les fichiers épinglés restent en tête et ne comptent pas dans la limite.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Nombre de fichiers non épinglés gardés.
pub const MAX: usize = 16;

/// Fichier récent : chemin canonique, dernière ouverture (secondes depuis 1970), épinglé.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recent {
    pub path: PathBuf,
    #[serde(default)]
    pub opened: u64,
    #[serde(default)]
    pub pinned: bool,
}

impl Recent {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Recent { path: path.into(), opened: 0, pinned: false }
    }

    pub fn name(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }
}

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("Obview 3D").join("recents.json"))
}

/// Liste enregistrée ; l'ancienne forme (chemins seuls, jusqu'à la 0.3) est reprise.
pub fn load() -> Vec<Recent> {
    let Some(b) = path().and_then(|p| std::fs::read(p).ok()) else { return Vec::new() };
    parse(&b)
}

fn parse(b: &[u8]) -> Vec<Recent> {
    if let Ok(v) = serde_json::from_slice::<Vec<Recent>>(b) {
        return v;
    }
    serde_json::from_slice::<Vec<PathBuf>>(b).map(|v| v.into_iter().map(Recent::new).collect()).unwrap_or_default()
}

pub fn save(list: &[Recent]) -> std::io::Result<()> {
    let Some(p) = path() else { return Ok(()) };
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(list).map_err(std::io::Error::other)?)?;
    std::fs::rename(tmp, p)
}

/// Secondes depuis 1970.
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Note l'ouverture de `file` : en tête de la liste (épinglé, il garde son épingle), sans doublon.
pub fn push(list: &mut Vec<Recent>, file: &Path) {
    let file = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let pinned = list.iter().any(|r| r.path == file && r.pinned);
    list.retain(|r| r.path != file);
    list.insert(0, Recent { path: file, opened: now(), pinned });
    trim(list);
}

/// Garde tous les épinglés et les [`MAX`] plus récents des autres.
fn trim(list: &mut Vec<Recent>) {
    let mut kept = 0;
    list.retain(|r| {
        if r.pinned {
            return true;
        }
        kept += 1;
        kept <= MAX
    });
}

/// Épingle ou désépingle `file`.
pub fn toggle_pin(list: &mut Vec<Recent>, file: &Path) {
    if let Some(r) = list.iter_mut().find(|r| r.path == file) {
        r.pinned = !r.pinned;
    }
    trim(list);
}

pub fn remove(list: &mut Vec<Recent>, file: &Path) {
    list.retain(|r| r.path != file);
}

/// Ordre de la page d'accueil : épinglés d'abord, puis les autres, chacun du plus récent au plus ancien.
pub fn ordered(list: &[Recent]) -> Vec<&Recent> {
    let mut v: Vec<&Recent> = list.iter().collect();
    v.sort_by_key(|r| (!r.pinned, std::cmp::Reverse(r.opened)));
    v
}

/// Type affiché d'un fichier : extension en capitales, formes longues ramenées au nom du format.
pub fn kind(p: &Path) -> String {
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "stp" | "step" => "STEP".into(),
        "igs" | "iges" => "IGES".into(),
        "gltf" => "glTF".into(),
        "" => "—".into(),
        e => e.to_ascii_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liste_sans_doublon() {
        let mut l = vec![Recent::new("/a.stl"), Recent::new("/b.step")];
        push(&mut l, Path::new("/b.step"));
        assert_eq!(l[0].path, PathBuf::from("/b.step"));
        assert_eq!(l.len(), 2);
        toggle_pin(&mut l, Path::new("/a.stl"));
        for i in 0..40 {
            push(&mut l, Path::new(&format!("/f{i}.obj")));
        }
        assert_eq!(l.len(), MAX + 1);
        assert!(l.iter().any(|r| r.path == Path::new("/a.stl") && r.pinned));
        assert_eq!(ordered(&l)[0].path, PathBuf::from("/a.stl"));
        assert_eq!(kind(Path::new("x.STP")), "STEP");
        assert_eq!(kind(Path::new("x.glb")), "GLB");
    }

    #[test]
    fn ancienne_forme() {
        let l = parse(br#"["/a.stl", "/b.step"]"#);
        assert_eq!(l, vec![Recent::new("/a.stl"), Recent::new("/b.step")]);
    }
}
