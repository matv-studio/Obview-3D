//! Mises à jour depuis l'application (D-32) : dernière version publiée du dépôt GitHub, fichier propre au système
//! téléchargé et vérifié par son empreinte SHA-256 (calculée par GitHub), puis installé en place et relancé.
//!
//! Fichiers attendus dans une version : `…-windows-x64-setup.exe` (Inno Setup), `…-macos-arm64.dmg`,
//! `…-linux-x86_64.AppImage`, `…_amd64.deb`.

use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use sha2::{Digest, Sha256};

/// Dépôt des versions.
pub const REPO: &str = "matv-studio/Obview-3D";
/// Version de cette construction.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Numéro de version `majeur.mineur.correctif` (les suffixes `-…` et `+…` sont ignorés).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u32, pub u32, pub u32);

impl Version {
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim().trim_start_matches(['v', 'V']);
        let core = s.split(['-', '+']).next()?;
        let mut it = core.split('.');
        let a = it.next()?.parse().ok()?;
        let b = it.next().unwrap_or("0").parse().ok()?;
        let c = it.next().unwrap_or("0").parse().ok()?;
        it.next().is_none().then_some(Version(a, b, c))
    }

    pub fn current() -> Version {
        Version::parse(CURRENT).unwrap_or(Version(0, 0, 0))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// Fichier d'une version.
#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// Empreinte SHA-256 en hexadécimal minuscule, donnée par GitHub.
    pub sha256: Option<String>,
}

/// Version plus récente que celle-ci.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: Version,
    /// Page de la version (notes).
    pub page: String,
    /// Fichier installable sur ce système ; absent : la page s'ouvre dans le navigateur.
    pub asset: Option<Asset>,
}

/// Façon dont l'application en cours a été installée, donc dont elle se remplace.
#[derive(Clone, Debug, PartialEq)]
pub enum Install {
    /// Windows, par l'installateur Inno Setup ; `all_users` : dans Program Files (élévation demandée).
    WindowsSetup { all_users: bool },
    /// macOS : paquet `.app` remplacé depuis l'image disque.
    MacBundle(PathBuf),
    /// Linux : fichier AppImage remplacé.
    AppImage(PathBuf),
    /// Linux : paquet Debian installé par `pkexec apt-get`.
    Deb,
    /// Construction de développement ou archive : la page de la version s'ouvre.
    Manual,
}

impl Install {
    /// Suffixe du fichier attendu pour ce mode d'installation.
    fn suffix(&self) -> Option<&'static str> {
        match self {
            Install::WindowsSetup { .. } => Some("-windows-x64-setup.exe"),
            Install::MacBundle(_) => Some("-macos-arm64.dmg"),
            Install::AppImage(_) => Some("-linux-x86_64.AppImage"),
            Install::Deb => Some("_amd64.deb"),
            Install::Manual => None,
        }
    }
}

/// Mode d'installation de l'exécutable en cours.
pub fn install_kind() -> Install {
    let Ok(exe) = std::env::current_exe() else { return Install::Manual };
    if cfg!(windows) {
        // L'installateur dépose son désinstalleur à côté de l'exécutable.
        let dir = exe.parent().unwrap_or(Path::new(""));
        if !dir.join("unins000.exe").exists() {
            return Install::Manual;
        }
        let under = |var: &str| std::env::var_os(var).is_some_and(|p| exe.starts_with(PathBuf::from(p)));
        return Install::WindowsSetup { all_users: under("ProgramFiles") || under("ProgramW6432") };
    }
    if cfg!(target_os = "macos") {
        let bundle = exe.parent().and_then(Path::parent).and_then(Path::parent);
        return match bundle {
            Some(b) if b.extension().is_some_and(|e| e == "app") => Install::MacBundle(b.to_path_buf()),
            _ => Install::Manual,
        };
    }
    if let Some(p) = std::env::var_os("APPIMAGE") {
        return Install::AppImage(PathBuf::from(p));
    }
    if exe.starts_with("/opt/obview-3d") { Install::Deb } else { Install::Manual }
}

fn agent() -> ureq::Agent {
    use ureq::tls::{RootCerts, TlsConfig};
    let config = ureq::Agent::config_builder()
        // Magasin de certificats du système : proxys d'entreprise compris.
        .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_response(Some(Duration::from_secs(20)))
        .user_agent(format!("Obview-3D/{CURRENT}"))
        .build();
    ureq::Agent::new_with_config(config)
}

/// Lit la réponse de `releases/latest` : une version plus récente que `current`, avec le fichier de `install`.
pub fn parse_latest(json: &str, current: Version, install: &Install) -> Result<Option<Release>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) {
        return Ok(None);
    }
    let tag = v["tag_name"].as_str().ok_or("tag_name absent")?;
    let Some(version) = Version::parse(tag) else { return Err(format!("numéro de version illisible : {tag}")) };
    if version <= current {
        return Ok(None);
    }
    let asset = install.suffix().and_then(|suffix| {
        v["assets"].as_array()?.iter().find_map(|a| {
            let name = a["name"].as_str()?;
            name.ends_with(suffix).then(|| Asset {
                name: name.to_string(),
                url: a["browser_download_url"].as_str().unwrap_or_default().to_string(),
                size: a["size"].as_u64().unwrap_or(0),
                sha256: a["digest"]
                    .as_str()
                    .and_then(|d| d.strip_prefix("sha256:"))
                    .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
                    .map(|h| h.to_ascii_lowercase()),
            })
        })
    });
    Ok(Some(Release { version, page: v["html_url"].as_str().unwrap_or_default().to_string(), asset }))
}

/// Interroge GitHub : `Some` si une version publiée est plus récente que celle-ci.
pub fn check() -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let r = agent()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call();
    let mut resp = match r {
        Ok(r) => r,
        // Aucune version publiée.
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let body = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
    parse_latest(&body, Version::current(), &install_kind())
}

/// Télécharge le fichier dans le dossier temporaire et vérifie son empreinte. `progress` reçoit la part reçue
/// (0 à 1) après chaque bloc.
pub fn download(asset: &Asset, progress: &mut dyn FnMut(f32), cancel: &AtomicBool) -> Result<PathBuf, String> {
    let Some(expected) = &asset.sha256 else { return Err("empreinte SHA-256 absente de la version".into()) };
    let dir = std::env::temp_dir().join("obview-3d-update");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(&asset.name);
    let part = dir.join(format!("{}.part", asset.name));
    let mut resp = agent().get(&asset.url).call().map_err(|e| e.to_string())?;
    let mut reader = resp.body_mut().with_config().limit(u64::MAX).reader();
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut got = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(&part);
            return Err("annulé".into());
        }
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        got += n as u64;
        if asset.size > 0 {
            progress((got as f64 / asset.size as f64).min(1.0) as f32);
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    let hex: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if &hex != expected {
        let _ = std::fs::remove_file(&part);
        return Err("empreinte SHA-256 différente : fichier rejeté".into());
    }
    std::fs::rename(&part, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Installe le fichier téléchargé. `Ok(true)` : l'installation est lancée ou faite, l'application doit se fermer
/// (elle est relancée) ; `Ok(false)` : le système a ouvert le fichier pour une installation à la main.
pub fn install(file: &Path, how: &Install) -> Result<bool, String> {
    match how {
        Install::WindowsSetup { all_users } => {
            // Inno Setup : progression visible, application fermée puis relancée par l'installateur (D-32).
            Command::new(file)
                .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS", "/RELAUNCH=1"])
                .arg(if *all_users { "/ALLUSERS" } else { "/CURRENTUSER" })
                .spawn()
                .map_err(|e| e.to_string())?;
            Ok(true)
        }
        Install::MacBundle(bundle) => match replace_bundle(file, bundle) {
            Ok(()) => {
                relaunch(bundle, true)?;
                Ok(true)
            }
            // Emplacement non modifiable (application lancée depuis l'image disque ou isolée par Gatekeeper) :
            // l'image disque s'ouvre pour un glisser vers Applications.
            Err(_) => open_path(file).map(|_| false),
        },
        Install::AppImage(target) => {
            let dir = target.parent().ok_or("dossier de l'AppImage introuvable")?;
            let staged = dir.join(".obview-3d-update.AppImage");
            std::fs::copy(file, &staged).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
            }
            std::fs::rename(&staged, target).map_err(|e| e.to_string())?;
            relaunch(target, false)?;
            Ok(true)
        }
        Install::Deb => {
            let ok = Command::new("pkexec")
                .args(["apt-get", "install", "-y", "--allow-downgrades"])
                .arg(file)
                .status()
                .is_ok_and(|s| s.success());
            if ok {
                relaunch(Path::new("/usr/bin/obview-3d"), false)?;
                Ok(true)
            } else {
                // Sans pkexec ou refusé : la logithèque du système ouvre le paquet.
                open_path(file).map(|_| false)
            }
        }
        Install::Manual => Err("installation automatique impossible pour cette copie".into()),
    }
}

/// Remplace le paquet `.app` par celui de l'image disque.
fn replace_bundle(dmg: &Path, bundle: &Path) -> Result<(), String> {
    if bundle.components().any(|c| c.as_os_str() == "AppTranslocation") {
        return Err("application isolée par Gatekeeper".into());
    }
    let parent = bundle.parent().ok_or("dossier de l'application introuvable")?;
    let mount = std::env::temp_dir().join(format!("obview-3d-dmg-{}", std::process::id()));
    std::fs::create_dir_all(&mount).map_err(|e| e.to_string())?;
    let run = |c: &mut Command| -> Result<(), String> {
        let s = c.status().map_err(|e| e.to_string())?;
        if s.success() { Ok(()) } else { Err(format!("{c:?} : {s}")) }
    };
    run(Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint"])
        .arg(&mount)
        .arg(dmg))?;
    let result = (|| {
        let src = std::fs::read_dir(&mount)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .ok_or("aucune application dans l'image disque")?;
        let staged = parent.join(".Obview 3D.update.app");
        let old = parent.join(".Obview 3D.old.app");
        let _ = std::fs::remove_dir_all(&staged);
        let _ = std::fs::remove_dir_all(&old);
        run(Command::new("ditto").arg(&src).arg(&staged))?;
        std::fs::rename(bundle, &old).map_err(|e| e.to_string())?;
        if let Err(e) = std::fs::rename(&staged, bundle) {
            let _ = std::fs::rename(&old, bundle);
            return Err(e.to_string());
        }
        let _ = std::fs::remove_dir_all(&old);
        Ok(())
    })();
    let _ = Command::new("hdiutil").args(["detach", "-quiet"]).arg(&mount).status();
    let _ = std::fs::remove_dir(&mount);
    result
}

/// Relance l'application une seconde après la fermeture de celle-ci.
fn relaunch(target: &Path, bundle: bool) -> Result<(), String> {
    let script = if bundle { "sleep 1; open \"$0\"" } else { "sleep 1; exec \"$0\" >/dev/null 2>&1" };
    Command::new("/bin/sh").args(["-c", script]).arg(target).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Ouvre un fichier ou une adresse avec l'application par défaut du système.
pub fn open_path(target: impl AsRef<std::ffi::OsStr>) -> Result<(), String> {
    let target = target.as_ref();
    let r = if cfg!(windows) {
        Command::new("explorer").arg(target).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(target).spawn()
    } else {
        Command::new("xdg-open").arg(target).spawn()
    };
    r.map(|_| ()).map_err(|e| e.to_string())
}

/// État des mises à jour montré par l'interface.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Status {
    #[default]
    Idle,
    /// Recherche en cours ; `manual` : demandée depuis les réglages (un échec s'affiche).
    Checking {
        manual: bool,
    },
    /// À jour après une recherche.
    Current,
    Available(Release),
    /// Téléchargement : part reçue, 0 à 1.
    Downloading(Release, f32),
    Installing(Release),
    Failed {
        release: Option<Release>,
        error: String,
    },
}

impl Status {
    /// Version proposée, s'il y en a une.
    pub fn release(&self) -> Option<&Release> {
        match self {
            Status::Available(r) | Status::Downloading(r, _) | Status::Installing(r) => Some(r),
            Status::Failed { release, .. } => release.as_ref(),
            _ => None,
        }
    }
}

/// Demande de l'interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Check,
    /// Télécharge et installe la version proposée (ou ouvre sa page sans fichier installable).
    Install,
    /// Ouvre la page de la version proposée (notes).
    OpenPage,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(Version::parse("v0.2.1"), Some(Version(0, 2, 1)));
        assert_eq!(Version::parse("1.4"), Some(Version(1, 4, 0)));
        assert_eq!(Version::parse("0.3.0-beta+7"), Some(Version(0, 3, 0)));
        assert_eq!(Version::parse("0.2.x"), None);
        assert!(Version(0, 10, 0) > Version(0, 9, 9));
    }

    const LATEST: &str = r#"{
        "tag_name": "v0.3.0", "draft": false, "prerelease": false,
        "html_url": "https://github.com/matv-studio/Obview-3D/releases/tag/v0.3.0",
        "assets": [
            {"name": "Obview-3D-0.3.0-windows-x64-setup.exe", "size": 100,
             "browser_download_url": "https://example.invalid/setup.exe",
             "digest": "sha256:ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789"},
            {"name": "Obview-3D-0.3.0-macos-arm64.dmg", "size": 200,
             "browser_download_url": "https://example.invalid/app.dmg", "digest": null}
        ]
    }"#;

    #[test]
    fn lecture_de_la_derniere_version() {
        let win = Install::WindowsSetup { all_users: false };
        let r = parse_latest(LATEST, Version(0, 2, 1), &win).unwrap().unwrap();
        assert_eq!(r.version, Version(0, 3, 0));
        let a = r.asset.unwrap();
        assert_eq!(a.name, "Obview-3D-0.3.0-windows-x64-setup.exe");
        assert_eq!(a.sha256.as_deref(), Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"));
        // Empreinte absente : fichier trouvé, téléchargement refusé plus tard.
        let mac = parse_latest(LATEST, Version(0, 2, 1), &Install::MacBundle(PathBuf::new())).unwrap().unwrap();
        assert_eq!(mac.asset.unwrap().sha256, None);
        // Pas de fichier pour ce système : la page seule.
        let deb = parse_latest(LATEST, Version(0, 2, 1), &Install::Deb).unwrap().unwrap();
        assert!(deb.asset.is_none());
        // Déjà à jour.
        assert_eq!(parse_latest(LATEST, Version(0, 3, 0), &win).unwrap(), None);
    }

    /// Interroge réellement GitHub (réseau) : `cargo test -p obview --lib recherche_reelle -- --ignored`.
    #[test]
    #[ignore]
    fn recherche_reelle() {
        let r = check();
        eprintln!("{r:?}");
        assert!(r.is_ok(), "{r:?}");
    }

    #[test]
    fn empreinte_verifiee() {
        let cancel = AtomicBool::new(true);
        let a = Asset { name: "x".into(), url: String::new(), size: 0, sha256: None };
        assert!(download(&a, &mut |_| {}, &cancel).is_err());
    }
}
