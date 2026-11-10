//! Langue de l'interface (D-29) : textes écrits en français dans le code, traduits à l'affichage.
//!
//! - [`tr`] rend la traduction d'un texte fixe dans la langue active ; les tables de traduction sont fournies
//!   par chaque crate ([`register`]) : la clé est le texte français, la valeur sa traduction anglaise.
//! - La langue est propre au fil (l'interface vit sur un seul fil ; les essais en parallèle ne se gênent pas).
//! - Nombres : séparateur des milliers et des décimales, espace avant les unités et « % », selon la langue.

use std::{
    cell::Cell,
    collections::HashMap,
    sync::{LazyLock, RwLock},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub enum Lang {
    #[default]
    En,
    Fr,
}

thread_local! {
    static LANG: Cell<Lang> = const { Cell::new(Lang::En) };
}

/// Traductions anglaises, par texte français.
static TABLE: LazyLock<RwLock<HashMap<&'static str, &'static str>>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    for (fr, en) in OWN {
        m.insert(*fr, *en);
    }
    RwLock::new(m)
});

/// Textes de cette bibliothèque.
pub const OWN: &[(&str, &str)] = &[
    ("Masquer", "Hide"),
    ("Afficher", "Show"),
    ("Effacer", "Clear"),
    ("Fermer", "Close"),
    ("Échap", "Esc"),
    ("Bleu", "Blue"),
    ("Violet", "Purple"),
    ("Rose", "Pink"),
    ("Rouge", "Red"),
    ("Orange", "Orange"),
    ("Jaune", "Yellow"),
    ("Vert", "Green"),
    ("Cyan", "Cyan"),
];

/// Ajoute des traductions (texte français, texte anglais).
pub fn register(pairs: &'static [(&'static str, &'static str)]) {
    let mut t = TABLE.write().unwrap_or_else(|e| e.into_inner());
    for (fr, en) in pairs {
        t.insert(*fr, *en);
    }
}

pub fn set_lang(l: Lang) {
    LANG.with(|c| c.set(l));
}

pub fn lang() -> Lang {
    LANG.with(Cell::get)
}

pub fn is_fr() -> bool {
    lang() == Lang::Fr
}

/// Texte fixe dans la langue active (le français s'il n'a pas de traduction).
pub fn tr(fr: &'static str) -> &'static str {
    if is_fr() {
        return fr;
    }
    TABLE.read().unwrap_or_else(|e| e.into_inner()).get(fr).copied().unwrap_or(fr)
}

/// Traduction connue de `fr` ? (essais de complétude des tables)
pub fn has_translation(fr: &str) -> bool {
    TABLE.read().unwrap_or_else(|e| e.into_inner()).contains_key(fr)
}

/// Message composé d'un début connu et d'un détail (« format non pris en charge : .xyz ») : le début est
/// traduit, le détail gardé tel quel.
pub fn tr_message(msg: &str) -> String {
    if is_fr() {
        return msg.to_owned();
    }
    let t = TABLE.read().unwrap_or_else(|e| e.into_inner());
    if let Some(en) = t.get(msg) {
        return (*en).to_owned();
    }
    // Début le plus long qui se termine par « : ».
    let mut best: Option<(&str, &str)> = None;
    for (fr, en) in t.iter() {
        if fr.ends_with(" :") && msg.starts_with(fr) && best.is_none_or(|(b, _)| fr.len() > b.len()) {
            best = Some((fr, en));
        }
    }
    match best {
        // Le détail peut lui-même commencer par un début connu (« fichier invalide : STL trop court »).
        Some((fr, en)) => {
            let rest = msg[fr.len()..].trim_start();
            drop(t);
            format!("{en} {}", tr_message(rest))
        }
        None => msg.to_owned(),
    }
}

/// Le premier texte si la langue est l'anglais, le second sinon (formes construites, pluriels).
pub fn pick<T>(en: T, fr: T) -> T {
    if is_fr() { fr } else { en }
}

/// Entier groupé par milliers : espace fine insécable en français, virgule en anglais.
pub fn group(n: u64) -> String {
    let s = n.to_string();
    let sep = if is_fr() { '\u{202f}' } else { ',' };
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(ch);
    }
    out
}

/// Nombre à `digits` décimales : virgule décimale en français, point en anglais.
pub fn decimal(v: f64, digits: usize) -> String {
    let s = format!("{v:.digits$}");
    if is_fr() { s.replace('.', ",") } else { s }
}

/// Valeur suivie de son unité : espace fine insécable en français (« 12 s », « 35 % »), espace simple en
/// anglais sauf avant « % » (« 12 s », « 35% »).
pub fn with_unit(value: &str, unit: &str) -> String {
    if is_fr() {
        format!("{value}\u{202f}{unit}")
    } else if unit == "%" {
        format!("{value}%")
    } else {
        format!("{value} {unit}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn langue_et_nombres() {
        set_lang(Lang::En);
        assert_eq!(tr("Fermer"), "Close");
        assert_eq!(tr("Texte inconnu"), "Texte inconnu");
        assert_eq!(group(1_234_567), "1,234,567");
        assert_eq!(decimal(1.25, 1), "1.2");
        assert_eq!(with_unit("35", "%"), "35%");
        register(&[
            ("format non pris en charge :", "unsupported format:"),
            ("fichier invalide :", "invalid file:"),
            ("STL trop court", "STL too short"),
        ]);
        assert_eq!(tr_message("format non pris en charge : .xyz"), "unsupported format: .xyz");
        assert_eq!(tr_message("fichier invalide : STL trop court"), "invalid file: STL too short");
        set_lang(Lang::Fr);
        assert_eq!(tr("Fermer"), "Fermer");
        assert_eq!(group(1_234_567), "1\u{202f}234\u{202f}567");
        assert_eq!(decimal(1.5, 1), "1,5");
        assert_eq!(with_unit("35", "%"), "35\u{202f}%");
        assert_eq!(tr_message("format non pris en charge : .xyz"), "format non pris en charge : .xyz");
    }
}
