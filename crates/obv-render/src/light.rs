//! Éclairage par environnement (D-33) : HDRI (Radiance .hdr, OpenEXR) ou environnements générés (studio, lumière
//! dure, contre-jour, couleurs), préfiltrés sur le processeur pour un rendu à base physique :
//!
//! - lumière diffuse : harmoniques sphériques d'ordre 2 (9 coefficients) de l'éclairement, divisé par π ;
//! - reflets : pyramide équirectangulaire préfiltrée par la distribution GGX (une rugosité par niveau) ;
//! - soleil : la zone très lumineuse et petite d'un HDRI devient une lumière directionnelle analytique (ombrage et
//!   reflet nets), retirée de l'image avant le préfiltrage ;
//! - lumières analytiques des environnements générés (au plus [`MAX_LIGHTS`]).
//!
//! Convention des directions (repère du monde, Y vers le haut) : u = atan2(x, −z) / 2π + ½, v = acos(y) / π
//! (v = 0 en haut). Le nuanceur (`scene_common.glsl`) emploie la même.

use std::f32::consts::{FRAC_1_PI, PI, TAU};

use bytemuck::Zeroable;
use rayon::prelude::*;

/// Lumières analytiques gérées par le nuanceur.
pub const MAX_LIGHTS: usize = 4;
/// Largeur du niveau 0 de la pyramide préfiltrée (hauteur : moitié).
pub const ENV_WIDTH: usize = 512;
/// Niveaux préfiltrés : rugosité `k / (ENV_LEVELS - 1)` au niveau `k`.
pub const ENV_LEVELS: usize = 6;

pub type V3 = [f32; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt().max(1e-12);
    mul(a, 1.0 / l)
}
fn luminance(c: V3) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Direction unitaire d'un point (u, v) de l'image équirectangulaire.
pub fn dir_of(u: f32, v: f32) -> V3 {
    let phi = (u - 0.5) * TAU;
    let theta = v * PI;
    [theta.sin() * phi.sin(), theta.cos(), -theta.sin() * phi.cos()]
}

/// Point (u, v) de l'image équirectangulaire vu dans la direction `d` (unitaire).
pub fn uv_of(d: V3) -> (f32, f32) {
    let u = d[0].atan2(-d[2]) / TAU + 0.5;
    let v = d[1].clamp(-1.0, 1.0).acos() / PI;
    (u, v)
}

/// Direction à partir d'angles en degrés : azimut 0 vers +Z (la caméra de la vue de face), positif vers +X (sa
/// droite), élévation au-dessus de l'horizon.
pub fn dir_deg(azimuth: f32, elevation: f32) -> V3 {
    let (a, e) = (azimuth.to_radians(), elevation.to_radians());
    [e.cos() * a.sin(), e.sin(), e.cos() * a.cos()]
}

/// Image en flottants, linéaire (radiance).
#[derive(Clone, Debug)]
pub struct HdrImage {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<V3>,
}

impl HdrImage {
    fn at(&self, x: isize, y: isize) -> V3 {
        let w = self.width as isize;
        let x = x.rem_euclid(w) as usize;
        let y = y.clamp(0, self.height as isize - 1) as usize;
        self.pixels[y * self.width + x]
    }

    /// Lecture bilinéaire, `u` bouclé, `v` borné.
    fn sample(&self, u: f32, v: f32) -> V3 {
        let x = u * self.width as f32 - 0.5;
        let y = v * self.height as f32 - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as isize, y0 as isize);
        let a = add(mul(self.at(x0, y0), 1.0 - fx), mul(self.at(x0 + 1, y0), fx));
        let b = add(mul(self.at(x0, y0 + 1), 1.0 - fx), mul(self.at(x0 + 1, y0 + 1), fx));
        add(mul(a, 1.0 - fy), mul(b, fy))
    }

    /// Réduction par moyenne de zones (rapports entiers ou non) à `w` × `h`.
    pub fn resize(&self, w: usize, h: usize) -> HdrImage {
        let (sx, sy) = (self.width as f32 / w as f32, self.height as f32 / h as f32);
        let pixels = (0..h)
            .into_par_iter()
            .flat_map_iter(|y| {
                (0..w).map(move |x| {
                    let (x0, x1) = ((x as f32 * sx) as usize, (((x + 1) as f32 * sx).ceil() as usize).max(1));
                    let (y0, y1) = ((y as f32 * sy) as usize, (((y + 1) as f32 * sy).ceil() as usize).max(1));
                    let mut acc = [0.0; 3];
                    let mut n = 0.0;
                    for yy in y0..y1.min(self.height) {
                        for xx in x0..x1.min(self.width) {
                            acc = add(acc, self.pixels[yy * self.width + xx]);
                            n += 1.0;
                        }
                    }
                    if n > 0.0 {
                        mul(acc, 1.0 / n)
                    } else {
                        self.sample((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32)
                    }
                })
            })
            .collect();
        HdrImage { width: w, height: h, pixels }
    }

    /// Angle solide d'un pixel de la ligne `y`.
    fn solid_angle(&self, y: usize) -> f32 {
        let theta = (y as f32 + 0.5) / self.height as f32 * PI;
        (TAU / self.width as f32) * (PI / self.height as f32) * theta.sin()
    }
}

/// Lit une image Radiance (.hdr, RGBE, avec ou sans compression RLE).
pub fn decode_hdr(bytes: &[u8]) -> Result<HdrImage, String> {
    let mut pos = 0;
    let mut line = || -> Option<&[u8]> {
        let start = pos;
        while pos < bytes.len() && bytes[pos] != b'\n' {
            pos += 1;
        }
        let l = &bytes[start..pos];
        pos += 1;
        (start < bytes.len()).then_some(l)
    };
    let first = line().ok_or("fichier HDR vide")?;
    if !first.starts_with(b"#?") {
        return Err("en-tête Radiance absent".into());
    }
    loop {
        let l = line().ok_or("en-tête HDR incomplet")?;
        if l.is_empty() {
            break;
        }
        if l.starts_with(b"FORMAT=") && l != b"FORMAT=32-bit_rle_rgbe" {
            return Err("format HDR non pris en charge (XYZE)".into());
        }
    }
    let res = String::from_utf8_lossy(line().ok_or("résolution HDR absente")?).into_owned();
    let parts: Vec<&str> = res.split_whitespace().collect();
    if parts.len() != 4 || parts[0] != "-Y" || parts[2] != "+X" {
        return Err(format!("orientation HDR non prise en charge : {res}"));
    }
    let height: usize = parts[1].parse().map_err(|_| "hauteur HDR illisible")?;
    let width: usize = parts[3].parse().map_err(|_| "largeur HDR illisible")?;
    if width == 0 || height == 0 || width > 32768 || height > 32768 {
        return Err("dimensions HDR invalides".into());
    }
    let data = &bytes[pos.min(bytes.len())..];
    let mut pixels = Vec::with_capacity(width * height);
    let mut i = 0;
    let mut scan = vec![[0u8; 4]; width];
    let take = |i: &mut usize| -> Result<u8, String> {
        let b = *data.get(*i).ok_or("données HDR tronquées")?;
        *i += 1;
        Ok(b)
    };
    for _ in 0..height {
        let rle = (8..32768).contains(&width)
            && data.get(i..i + 2) == Some(&[2, 2])
            && data.get(i + 2).is_some_and(|b| b & 0x80 == 0);
        if rle {
            let w = ((data[i + 2] as usize) << 8) | data[i + 3] as usize;
            if w != width {
                return Err("ligne HDR de largeur inattendue".into());
            }
            i += 4;
            for c in 0..4 {
                let mut x = 0;
                while x < width {
                    let n = take(&mut i)? as usize;
                    if n > 128 {
                        let count = n - 128;
                        let v = take(&mut i)?;
                        if x + count > width {
                            return Err("répétition HDR hors ligne".into());
                        }
                        for px in &mut scan[x..x + count] {
                            px[c] = v;
                        }
                        x += count;
                    } else {
                        if n == 0 || x + n > width {
                            return Err("suite HDR invalide".into());
                        }
                        for px in &mut scan[x..x + n] {
                            px[c] = take(&mut i)?;
                        }
                        x += n;
                    }
                }
            }
        } else {
            for px in scan.iter_mut() {
                for c in px.iter_mut() {
                    *c = take(&mut i)?;
                }
            }
        }
        pixels.extend(scan.iter().map(|p| {
            if p[3] == 0 {
                [0.0; 3]
            } else {
                let f = 2f32.powi(p[3] as i32 - 136);
                [p[0] as f32 * f, p[1] as f32 * f, p[2] as f32 * f]
            }
        }));
    }
    Ok(HdrImage { width, height, pixels })
}

/// Lit une image OpenEXR (premier calque RGB(A)).
pub fn decode_exr(path: &std::path::Path) -> Result<HdrImage, String> {
    use exr::prelude::*;
    let img = read_first_rgba_layer_from_file(
        path,
        |res, _| HdrImage {
            width: res.width(),
            height: res.height(),
            pixels: vec![[0.0; 3]; res.width() * res.height()],
        },
        |img: &mut HdrImage, pos, (r, g, b, _a): (f32, f32, f32, f32)| {
            let w = img.width;
            img.pixels[pos.y() * w + pos.x()] = [r.max(0.0), g.max(0.0), b.max(0.0)];
        },
    )
    .map_err(|e| e.to_string())?;
    Ok(img.layer_data.channel_data.pixels)
}

/// Lit un HDRI selon son extension (.hdr, .exr).
pub fn load_hdri(path: &std::path::Path) -> Result<HdrImage, String> {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("exr") => decode_exr(path),
        _ => decode_hdr(&std::fs::read(path).map_err(|e| e.to_string())?),
    }
}

/// Lumière directionnelle : direction vers la lumière (repère de l'environnement), éclairement (couleur), demi-
/// angle apparent (radians, adoucit le reflet).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirLight {
    pub dir: V3,
    pub irradiance: V3,
    pub angle: f32,
}

/// Panneau lumineux d'un environnement généré : centre, demi-tailles angulaires (degrés), radiance, adoucissement
/// du bord (degrés).
#[derive(Clone, Copy, Debug)]
pub struct Softbox {
    pub dir: V3,
    pub half: [f32; 2],
    pub radiance: V3,
    pub soft: f32,
}

/// Environnement généré : dôme en dégradé (haut, horizon, sol), panneaux lumineux, lumières analytiques.
#[derive(Clone, Debug)]
pub struct Procedural {
    pub top: V3,
    pub horizon: V3,
    pub ground: V3,
    pub softboxes: Vec<Softbox>,
    pub lights: Vec<DirLight>,
}

impl Procedural {
    fn radiance(&self, d: V3) -> V3 {
        let y = d[1];
        let mut c = if y >= 0.0 {
            let t = y.powf(0.6);
            add(mul(self.horizon, 1.0 - t), mul(self.top, t))
        } else {
            let t = (-y * 6.0).min(1.0);
            add(mul(self.horizon, 1.0 - t), mul(self.ground, t))
        };
        for s in &self.softboxes {
            let f = dot(d, s.dir);
            if f <= 0.0 {
                continue;
            }
            let up0 = if s.dir[1].abs() > 0.95 { [0.0, 0.0, -1.0] } else { [0.0, 1.0, 0.0] };
            let right = norm(cross(s.dir, up0));
            let up = cross(right, s.dir);
            let x = dot(d, right).atan2(f).to_degrees().abs();
            let yy = dot(d, up).atan2(f).to_degrees().abs();
            let edge = |a: f32, h: f32| {
                let t = ((h + s.soft - a) / (2.0 * s.soft).max(1e-3)).clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            };
            let m = edge(x, s.half[0]) * edge(yy, s.half[1]);
            if m > 0.0 {
                c = add(c, mul(s.radiance, m));
            }
        }
        c
    }

    /// Image équirectangulaire de l'environnement (`w` × `w/2`).
    pub fn render(&self, w: usize) -> HdrImage {
        let h = w / 2;
        let pixels = (0..h)
            .into_par_iter()
            .flat_map_iter(|y| {
                (0..w).map(move |x| {
                    // Quatre échantillons par pixel : bords des panneaux sans crénelage.
                    let mut acc = [0.0; 3];
                    for (ox, oy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                        let d = dir_of((x as f32 + ox) / w as f32, (y as f32 + oy) / h as f32);
                        acc = add(acc, self.radiance(d));
                    }
                    mul(acc, 0.25)
                })
            })
            .collect();
        HdrImage { width: w, height: h, pixels }
    }
}

/// Environnement préfiltré, prêt à envoyer au GPU.
#[derive(Clone, Debug)]
pub struct Environment {
    /// Niveaux de la pyramide (niveau `k` : `ENV_WIDTH >> k` × moitié), RGBA en demi-flottants.
    pub levels: Vec<(u32, u32, Vec<u16>)>,
    /// Harmoniques sphériques de l'éclairement divisé par π (9 coefficients RGB).
    pub sh: [V3; 9],
    pub lights: Vec<DirLight>,
    /// Gain qui ramène l'environnement à une luminosité de présentation commune.
    pub gain: f32,
    /// Rotation de départ du dôme (radians) : lumière principale d'un HDRI devant à gauche.
    pub base_rotation: f32,
}

/// Extrait le soleil d'un HDRI : zone petite (moins de `0,01` sr) et très lumineuse (plus de 60 fois la médiane),
/// portant au moins le quart de l'éclairement. La zone est ramenée au seuil dans l'image.
pub fn extract_sun(img: &mut HdrImage) -> Option<DirLight> {
    let lum: Vec<f32> = img.pixels.iter().map(|&c| luminance(c)).collect();
    let mut sorted = lum.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let median = sorted[sorted.len() / 2].max(1e-6);
    let (imax, &lmax) = lum.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
    let threshold = median * 60.0;
    if lmax < threshold {
        return None;
    }
    let (w, h) = (img.width, img.height);
    let center = dir_of(((imax % w) as f32 + 0.5) / w as f32, ((imax / w) as f32 + 0.5) / h as f32);
    let cos_r = 12f32.to_radians().cos();
    let mut energy = [0.0; 3];
    let mut weighted = [0.0; 3];
    let mut solid = 0.0;
    let mut total = 0.0;
    let mut hot = Vec::new();
    for y in 0..h {
        let sa = img.solid_angle(y);
        for x in 0..w {
            let k = y * w + x;
            total += lum[k] * sa;
            if lum[k] <= threshold {
                continue;
            }
            let d = dir_of((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            if dot(d, center) < cos_r {
                continue;
            }
            let excess = mul(img.pixels[k], 1.0 - threshold / lum[k]);
            energy = add(energy, mul(excess, sa));
            weighted = add(weighted, mul(d, luminance(excess) * sa));
            solid += sa;
            hot.push(k);
        }
    }
    if solid > 0.01 || luminance(energy) < 0.25 * total {
        return None;
    }
    for k in hot {
        let l = lum[k];
        img.pixels[k] = mul(img.pixels[k], threshold / l);
    }
    let angle = (solid / PI).sqrt().max(0.5f32.to_radians());
    Some(DirLight { dir: norm(weighted), irradiance: energy, angle })
}

/// Harmoniques sphériques (ordre 2) de l'éclairement divisé par π.
pub fn sh_irradiance(img: &HdrImage) -> [V3; 9] {
    let mut sh = [[0.0f32; 3]; 9];
    for y in 0..img.height {
        let sa = img.solid_angle(y);
        for x in 0..img.width {
            let d = dir_of((x as f32 + 0.5) / img.width as f32, (y as f32 + 0.5) / img.height as f32);
            let c = mul(img.pixels[y * img.width + x], sa);
            for (i, b) in sh_basis(d).iter().enumerate() {
                sh[i] = add(sh[i], mul(c, *b));
            }
        }
    }
    // Convolution par le lobe en cosinus (Ramamoorthi et Hanrahan), puis division par π.
    let band = [PI, 2.0 * PI / 3.0, 2.0 * PI / 3.0, 2.0 * PI / 3.0, PI / 4.0, PI / 4.0, PI / 4.0, PI / 4.0, PI / 4.0];
    for i in 0..9 {
        sh[i] = mul(sh[i], band[i] * FRAC_1_PI);
    }
    sh
}

/// Base des harmoniques sphériques réelles d'ordre 2 (même ordre que `sh_eval` du nuanceur).
pub fn sh_basis(d: V3) -> [f32; 9] {
    let [x, y, z] = d;
    [
        0.282_095,
        0.488_603 * y,
        0.488_603 * z,
        0.488_603 * x,
        1.092_548 * x * y,
        1.092_548 * y * z,
        0.315_392 * (3.0 * z * z - 1.0),
        1.092_548 * x * z,
        0.546_274 * (x * x - y * y),
    ]
}

/// Évalue les harmoniques en `n`.
pub fn sh_eval(sh: &[V3; 9], n: V3) -> V3 {
    sh_basis(n).iter().zip(sh).fold([0.0; 3], |acc, (b, c)| add(acc, mul(*c, *b)))
}

fn radical_inverse(mut b: u32) -> f32 {
    b = b.rotate_right(16);
    b = ((b & 0x5555_5555) << 1) | ((b & 0xAAAA_AAAA) >> 1);
    b = ((b & 0x3333_3333) << 2) | ((b & 0xCCCC_CCCC) >> 2);
    b = ((b & 0x0F0F_0F0F) << 4) | ((b & 0xF0F0_F0F0) >> 4);
    b = ((b & 0x00FF_00FF) << 8) | ((b & 0xFF00_FF00) >> 8);
    b as f32 * 2.328_306_4e-10
}

/// Pyramide des moyennes (pour l'échantillonnage filtré selon la densité, Křivánek et Colbert).
fn box_pyramid(base: &HdrImage) -> Vec<HdrImage> {
    let mut v = vec![base.clone()];
    while v.last().is_some_and(|i| i.width > 2) {
        let l = v.last().unwrap();
        v.push(l.resize(l.width / 2, (l.height / 2).max(1)));
    }
    v
}

fn sample_pyramid(p: &[HdrImage], d: V3, lod: f32) -> V3 {
    let (u, v) = uv_of(d);
    let lod = lod.clamp(0.0, (p.len() - 1) as f32);
    let l0 = lod.floor() as usize;
    let l1 = (l0 + 1).min(p.len() - 1);
    let t = lod - l0 as f32;
    add(mul(p[l0].sample(u, v), 1.0 - t), mul(p[l1].sample(u, v), t))
}

/// Préfiltre GGX d'un niveau : chaque pixel intègre l'environnement autour de sa direction (N = V = R).
fn prefilter_level(p: &[HdrImage], w: usize, roughness: f32, samples: u32) -> HdrImage {
    let h = w / 2;
    let a = roughness * roughness;
    let base = &p[0];
    let texel_sa = 4.0 * PI / (base.width * base.height) as f32;
    let pixels = (0..h)
        .into_par_iter()
        .flat_map_iter(|y| {
            (0..w).map(move |x| {
                let n = dir_of((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
                let up0 = if n[1].abs() < 0.999 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
                let tx = norm(cross(up0, n));
                let ty = cross(n, tx);
                let mut acc = [0.0; 3];
                let mut wsum = 0.0;
                for i in 0..samples {
                    let (e1, e2) = (i as f32 / samples as f32, radical_inverse(i));
                    let phi = TAU * e1;
                    let cos_t = ((1.0 - e2) / (1.0 + (a * a - 1.0) * e2)).sqrt();
                    let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
                    let hdir = add(add(mul(tx, sin_t * phi.cos()), mul(ty, sin_t * phi.sin())), mul(n, cos_t));
                    let l = add(mul(hdir, 2.0 * dot(n, hdir)), mul(n, -1.0));
                    let nl = dot(n, l);
                    if nl <= 0.0 {
                        continue;
                    }
                    // Densité GGX de la direction réfléchie (N = V) : D(h) / 4.
                    let d2 = cos_t * cos_t * (a * a - 1.0) + 1.0;
                    let dd = a * a / (PI * d2 * d2);
                    let pdf = dd / 4.0;
                    let sa = 1.0 / (samples as f32 * pdf + 1e-6);
                    let lod = 0.5 * (sa / texel_sa).log2() + 1.0;
                    acc = add(acc, mul(sample_pyramid(p, l, lod), nl));
                    wsum += nl;
                }
                mul(acc, 1.0 / wsum.max(1e-6))
            })
        })
        .collect();
    HdrImage { width: w, height: h, pixels }
}

fn half_bits(v: f32) -> u16 {
    half::f16::from_f32(v).to_bits()
}

/// Préfiltre un environnement : soleil extrait (si `find_sun`), harmoniques, pyramide GGX, gain de présentation.
pub fn prepare(mut img: HdrImage, mut lights: Vec<DirLight>, find_sun: bool) -> Environment {
    // Les HDRI de plus de 2048 px de large sont d'abord réduits : le préfiltrage ne lit pas plus fin.
    if img.width > 2048 {
        img = img.resize(2048, 1024);
    }
    if find_sun
        && lights.len() < MAX_LIGHTS
        && let Some(sun) = extract_sun(&mut img)
    {
        lights.insert(0, sun);
    }
    lights.truncate(MAX_LIGHTS);
    let base = img.resize(ENV_WIDTH, ENV_WIDTH / 2);
    let sh = sh_irradiance(&base.resize(128, 64));
    let pyramid = box_pyramid(&base);
    let mut levels = Vec::with_capacity(ENV_LEVELS);
    for k in 0..ENV_LEVELS {
        let w = ENV_WIDTH >> k;
        let lvl =
            if k == 0 { base.clone() } else { prefilter_level(&pyramid, w, k as f32 / (ENV_LEVELS - 1) as f32, 96) };
        let mut data = Vec::with_capacity(lvl.pixels.len() * 4);
        for c in &lvl.pixels {
            data.extend_from_slice(&[half_bits(c[0]), half_bits(c[1]), half_bits(c[2]), half_bits(1.0)]);
        }
        levels.push((lvl.width as u32, lvl.height as u32, data));
    }
    // Gain : une surface blanche tournée vers la lumière la plus forte atteint 0,85 (avant la correspondance de
    // tons) ; directions tirées sur une spirale de Fibonacci.
    let irradiance = |d: V3| {
        let mut e = luminance(sh_eval(&sh, d)).max(0.0);
        for l in &lights {
            e += luminance(l.irradiance) * dot(d, l.dir).max(0.0) * FRAC_1_PI;
        }
        e
    };
    let n = 512;
    let mut max_e: f32 = 0.0;
    let mut best = [0.0, 1.0, 0.0];
    for i in 0..n {
        let y = 1.0 - 2.0 * (i as f32 + 0.5) / n as f32;
        let r = (1.0 - y * y).max(0.0).sqrt();
        let a = i as f32 * 2.399_963;
        let d = [r * a.sin(), y, r * a.cos()];
        let e = irradiance(d);
        if e > max_e {
            max_e = e;
            best = d;
        }
    }
    let gain = if max_e > 1e-6 { 0.85 / max_e } else { 1.0 };
    // HDRI : la direction la plus lumineuse est amenée devant à gauche de la vue de face (azimut −40°).
    let base_rotation = if find_sun { (-40f32).to_radians() - best[0].atan2(best[2]) } else { 0.0 };
    Environment { levels, sh, lights, gain, base_rotation }
}

/// Scénario d'éclairage proposé.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Preset {
    Studio,
    StudioPhoto,
    StudioWarm,
    Interior,
    Outdoor,
    Sunset,
    Hard,
    Rim,
    Neon,
    WarmCool,
}

impl Preset {
    pub const ALL: [Preset; 10] = [
        Preset::Studio,
        Preset::StudioPhoto,
        Preset::StudioWarm,
        Preset::Interior,
        Preset::Outdoor,
        Preset::Sunset,
        Preset::Hard,
        Preset::Rim,
        Preset::Neon,
        Preset::WarmCool,
    ];

    /// Nom français (traduit par l'application).
    pub fn label(self) -> &'static str {
        match self {
            Preset::Studio => "Studio",
            Preset::StudioPhoto => "Studio photo",
            Preset::StudioWarm => "Studio chaud",
            Preset::Interior => "Intérieur",
            Preset::Outdoor => "Extérieur",
            Preset::Sunset => "Coucher de soleil",
            Preset::Hard => "Lumière dure",
            Preset::Rim => "Contre-jour",
            Preset::Neon => "Néon",
            Preset::WarmCool => "Chaud et froid",
        }
    }

    /// Identifiant stable (réglages).
    pub fn key(self) -> &'static str {
        match self {
            Preset::Studio => "studio",
            Preset::StudioPhoto => "studio-photo",
            Preset::StudioWarm => "studio-warm",
            Preset::Interior => "interior",
            Preset::Outdoor => "outdoor",
            Preset::Sunset => "sunset",
            Preset::Hard => "hard",
            Preset::Rim => "rim",
            Preset::Neon => "neon",
            Preset::WarmCool => "warm-cool",
        }
    }

    pub fn from_key(k: &str) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| p.key() == k)
    }

    /// HDRI livré (Poly Haven, CC0) ou environnement généré.
    pub fn source(self) -> Source {
        match self {
            Preset::StudioPhoto => Source::Hdri(include_bytes!("../assets/hdri/studio_small_09_1k.hdr")),
            Preset::StudioWarm => Source::Hdri(include_bytes!("../assets/hdri/brown_photostudio_02_1k.hdr")),
            Preset::Interior => Source::Hdri(include_bytes!("../assets/hdri/lebombo_1k.hdr")),
            Preset::Outdoor => {
                Source::Hdri(include_bytes!("../assets/hdri/kloofendal_48d_partly_cloudy_puresky_1k.hdr"))
            }
            Preset::Sunset => Source::Hdri(include_bytes!("../assets/hdri/venice_sunset_1k.hdr")),
            p => Source::Procedural(p.procedural()),
        }
    }

    fn procedural(self) -> Procedural {
        let grey = |v: f32| [v, v, v];
        let sb = |az: f32, el: f32, w: f32, h: f32, c: V3, k: f32| Softbox {
            dir: dir_deg(az, el),
            half: [w, h],
            radiance: mul(c, k),
            soft: 3.0,
        };
        let white = [1.0, 1.0, 1.0];
        match self {
            // Trois panneaux (principal, appoint, bandeau haut) et une bordure arrière sur un fond gris sombre.
            Preset::Studio => Procedural {
                top: grey(0.10),
                horizon: grey(0.06),
                ground: grey(0.035),
                softboxes: vec![
                    sb(-40.0, 30.0, 18.0, 22.0, white, 9.0),
                    sb(55.0, 15.0, 14.0, 18.0, white, 3.0),
                    sb(0.0, 80.0, 40.0, 6.0, white, 5.0),
                    sb(170.0, 25.0, 6.0, 30.0, white, 6.0),
                ],
                lights: Vec::new(),
            },
            // Une source ponctuelle forte et un fond presque noir : ombres propres nettes, reflets francs.
            Preset::Hard => Procedural {
                top: grey(0.035),
                horizon: grey(0.02),
                ground: grey(0.01),
                softboxes: vec![sb(60.0, 10.0, 25.0, 15.0, white, 0.25)],
                lights: vec![DirLight { dir: dir_deg(-45.0, 40.0), irradiance: grey(7.0), angle: 0.6f32.to_radians() }],
            },
            // Deux bandeaux derrière l'objet, appoint faible devant.
            Preset::Rim => Procedural {
                top: grey(0.025),
                horizon: grey(0.015),
                ground: grey(0.01),
                softboxes: vec![
                    sb(150.0, 20.0, 5.0, 35.0, white, 14.0),
                    sb(-150.0, 20.0, 5.0, 35.0, white, 14.0),
                    sb(0.0, 10.0, 25.0, 15.0, white, 0.6),
                ],
                lights: Vec::new(),
            },
            // Panneaux saturés : magenta et cyan latéraux, bordure bleue.
            Preset::Neon => Procedural {
                top: [0.02, 0.015, 0.035],
                horizon: [0.015, 0.01, 0.025],
                ground: [0.008, 0.006, 0.012],
                softboxes: vec![
                    sb(-70.0, 15.0, 8.0, 30.0, [1.0, 0.12, 0.65], 9.0),
                    sb(70.0, 15.0, 8.0, 30.0, [0.05, 0.85, 1.0], 9.0),
                    sb(180.0, 35.0, 30.0, 6.0, [0.25, 0.3, 1.0], 6.0),
                    sb(0.0, 60.0, 20.0, 10.0, white, 1.2),
                ],
                lights: Vec::new(),
            },
            // Principal chaud (3200 K environ), appoint froid (8000 K environ).
            Preset::WarmCool => Procedural {
                top: [0.05, 0.055, 0.07],
                horizon: [0.035, 0.035, 0.04],
                ground: grey(0.02),
                softboxes: vec![
                    sb(-45.0, 30.0, 16.0, 20.0, [1.0, 0.62, 0.32], 9.0),
                    sb(60.0, 10.0, 16.0, 20.0, [0.45, 0.62, 1.0], 5.0),
                    sb(175.0, 30.0, 6.0, 25.0, [1.0, 0.8, 0.6], 4.0),
                ],
                lights: Vec::new(),
            },
            _ => Preset::Studio.procedural(),
        }
    }
}

/// Origine d'un environnement.
pub enum Source {
    Hdri(&'static [u8]),
    Procedural(Procedural),
}

/// Prépare un scénario livré (décodage et préfiltrage : quelques centaines de millisecondes).
pub fn build_preset(p: Preset) -> Result<Environment, String> {
    match p.source() {
        Source::Hdri(bytes) => Ok(prepare(decode_hdr(bytes)?, Vec::new(), true)),
        Source::Procedural(pr) => Ok(prepare(pr.render(1024), pr.lights.clone(), false)),
    }
}

/// Prépare un HDRI choisi par l'utilisateur (.hdr ou .exr).
pub fn build_file(path: &std::path::Path) -> Result<Environment, String> {
    Ok(prepare(load_hdri(path)?, Vec::new(), true))
}

/// Aperçu d'un environnement : panorama `w` × `h` en sRGB 8 bits, exposé pour une luminance moyenne de 0,3 et
/// passé par la même correspondance de tons que la vue.
pub fn thumbnail(img: &HdrImage, w: usize, h: usize) -> Vec<[u8; 3]> {
    let small = img.resize(w, h);
    let mean = small.pixels.iter().map(|&c| luminance(c)).sum::<f32>() / small.pixels.len().max(1) as f32;
    let gain = if mean > 1e-6 { 0.3 / mean } else { 1.0 };
    small.pixels.iter().map(|&c| srgb8(tone_map(mul(c, gain)))).collect()
}

/// Aperçu d'un scénario livré.
pub fn preset_thumbnail(p: Preset, w: usize, h: usize) -> Result<Vec<[u8; 3]>, String> {
    let img = match p.source() {
        Source::Hdri(bytes) => decode_hdr(bytes)?,
        Source::Procedural(pr) => pr.render(w * 4),
    };
    Ok(thumbnail(&img, w, h))
}

/// Correspondance de tons « PBR Neutral » de Khronos (même formule que `light.glsl`).
pub fn tone_map(c: V3) -> V3 {
    let start = 0.8 - 0.04;
    let desat = 0.15;
    let x = c[0].min(c[1]).min(c[2]);
    let offset = if x < 0.08 { x - 6.25 * x * x } else { 0.04 };
    let c = [c[0] - offset, c[1] - offset, c[2] - offset];
    let peak = c[0].max(c[1]).max(c[2]);
    if peak < start {
        return c;
    }
    let d = 1.0 - start;
    let new_peak = 1.0 - d * d / (peak + d - start);
    let c = mul(c, new_peak / peak);
    let g = 1.0 - 1.0 / (desat * (peak - new_peak) + 1.0);
    add(mul(c, 1.0 - g), mul([new_peak; 3], g))
}

fn srgb8(c: V3) -> [u8; 3] {
    let f = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (s * 255.0 + 0.5) as u8
    };
    [f(c[0]), f(c[1]), f(c[2])]
}

/// Paramètres d'éclairage d'une image, lus par les nuanceurs (20 vec4, 320 octets).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightBlock {
    /// cos et sin de la rotation du dôme, gain total (exposition comprise), dernier niveau de la pyramide.
    pub rot: [f32; 4],
    pub sh: [[f32; 4]; 9],
    pub light_dir: [[f32; 4]; MAX_LIGHTS],
    pub light_color: [[f32; 4]; MAX_LIGHTS],
    /// x : fond (0 dégradé, 1 environnement), y : niveau de flou du fond, z : nombre de lumières, w : teinte
    /// appliquée (1) ou non (0).
    pub background: [f32; 4],
    /// Teinte multipliée à l'environnement et aux lumières (RGB), w inutilisé.
    pub tint: [f32; 4],
}

/// Réglages d'éclairage d'une image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lighting {
    /// Rotation du dôme autour de la verticale (radians).
    pub rotation: f32,
    /// Exposition (IL) ajoutée au gain de présentation.
    pub exposure: f32,
    /// Fond : environnement visible (sinon dégradé du thème) et flou (0 net à 1 très flou).
    pub show_background: bool,
    pub blur: f32,
    /// Teinte (RGB linéaire) appliquée à toute la lumière.
    pub tint: V3,
}

impl Default for Lighting {
    fn default() -> Self {
        Lighting { rotation: 0.0, exposure: 0.0, show_background: false, blur: 0.5, tint: [1.0; 3] }
    }
}

impl Default for LightBlock {
    fn default() -> Self {
        LightBlock::neutral()
    }
}

impl LightBlock {
    pub fn new(env: &Environment, l: &Lighting) -> LightBlock {
        let mut b = LightBlock::zeroed();
        let gain = env.gain * 2f32.powf(l.exposure);
        let r = env.base_rotation + l.rotation;
        b.rot = [r.cos(), r.sin(), gain, (ENV_LEVELS - 1) as f32];
        for (i, c) in env.sh.iter().enumerate() {
            b.sh[i] = [c[0], c[1], c[2], 0.0];
        }
        for (i, li) in env.lights.iter().take(MAX_LIGHTS).enumerate() {
            b.light_dir[i] = [li.dir[0], li.dir[1], li.dir[2], li.angle];
            b.light_color[i] = [li.irradiance[0], li.irradiance[1], li.irradiance[2], 0.0];
        }
        b.background = [
            l.show_background as u32 as f32,
            l.blur.clamp(0.0, 1.0) * (ENV_LEVELS - 1) as f32,
            env.lights.len().min(MAX_LIGHTS) as f32,
            1.0,
        ];
        b.tint = [l.tint[0], l.tint[1], l.tint[2], 0.0];
        b
    }

    /// Bloc neutre (avant le premier environnement) : lumière ambiante grise uniforme.
    pub fn neutral() -> LightBlock {
        let mut b = LightBlock::zeroed();
        b.rot = [1.0, 0.0, 1.0, 0.0];
        b.sh[0] = [0.55 / 0.282_095, 0.55 / 0.282_095, 0.55 / 0.282_095, 0.0];
        b.tint = [1.0, 1.0, 1.0, 0.0];
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_et_coordonnees() {
        for d in [[0.0, 0.0, -1.0], [1.0, 0.0, 0.0], norm([0.3, 0.8, -0.5]), norm([-0.6, -0.2, 0.7])] {
            let (u, v) = uv_of(d);
            let e = dir_of(u, v);
            assert!((dot(d, e) - 1.0).abs() < 1e-5, "{d:?} → {e:?}");
        }
        assert!((uv_of([0.0, 0.0, -1.0]).0 - 0.5).abs() < 1e-6);
        assert!(uv_of([0.0, 1.0, 0.0]).1 < 1e-6);
    }

    #[test]
    fn harmoniques_d_un_ciel_uniforme() {
        // Radiance 1 partout : éclairement π, divisé par π : 1 dans toutes les directions.
        let img = HdrImage { width: 64, height: 32, pixels: vec![[1.0; 3]; 64 * 32] };
        let sh = sh_irradiance(&img);
        for d in [[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, -1.0, 0.0]] {
            let e = sh_eval(&sh, d);
            assert!((e[0] - 1.0).abs() < 0.02, "{e:?}");
        }
    }

    #[test]
    fn hdr_livres_lisibles() {
        for p in Preset::ALL {
            if let Source::Hdri(b) = p.source() {
                let img = decode_hdr(b).unwrap();
                assert_eq!((img.width, img.height), (1024, 512), "{p:?}");
                assert!(img.pixels.iter().all(|c| c.iter().all(|v| v.is_finite() && *v >= 0.0)));
            }
        }
    }

    #[test]
    fn soleil_du_ciel_exterieur() {
        let mut img = decode_hdr(match Preset::Outdoor.source() {
            Source::Hdri(b) => b,
            Source::Procedural(_) => unreachable!(),
        })
        .unwrap();
        let sun = extract_sun(&mut img).expect("soleil attendu");
        assert!(sun.dir[1] > 0.2, "soleil sous l'horizon : {:?}", sun.dir);
    }

    #[test]
    fn scenarios_prepares() {
        for p in [Preset::Studio, Preset::Hard] {
            let env = build_preset(p).unwrap();
            assert_eq!(env.levels.len(), ENV_LEVELS);
            assert_eq!(env.levels[0].0 as usize, ENV_WIDTH);
            assert!(env.gain.is_finite() && env.gain > 0.0);
        }
    }
}
