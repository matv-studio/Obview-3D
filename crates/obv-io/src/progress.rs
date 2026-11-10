//! Avancement d'une lecture, partagé entre le fil de travail et l'interface (sans verrou).

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

/// Étape d'une ouverture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Analyse du fichier.
    Read,
    /// Conversion des entités du fichier en formes (STEP / IGES).
    Transfer,
    /// Maillage des surfaces exactes (STEP / IGES).
    Mesh,
    /// Préparation de l'affichage.
    Prepare,
}

impl Stage {
    const ALL: [Stage; 4] = [Stage::Read, Stage::Transfer, Stage::Mesh, Stage::Prepare];
}

/// Étape en cours, fraction de l'étape (indéterminée tant qu'elle n'est pas mesurable) et demande d'arrêt.
#[derive(Debug)]
pub struct Progress {
    stage: AtomicU8,
    /// `f32` en bits ; NaN : indéterminée.
    fraction: AtomicU32,
    cancel: AtomicBool,
}

impl Default for Progress {
    fn default() -> Self {
        Progress {
            stage: AtomicU8::new(0),
            fraction: AtomicU32::new(f32::NAN.to_bits()),
            cancel: AtomicBool::new(false),
        }
    }
}

impl Progress {
    /// Passe à `stage`, fraction indéterminée.
    pub fn stage(&self, stage: Stage) {
        self.fraction.store(f32::NAN.to_bits(), Ordering::Relaxed);
        self.stage.store(stage as u8, Ordering::Relaxed);
    }

    /// Fraction de l'étape en cours (bornée à [0, 1]).
    pub fn set(&self, fraction: f32) {
        self.fraction.store(fraction.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Étape et fraction (`None` : indéterminée).
    pub fn get(&self) -> (Stage, Option<f32>) {
        let stage = Stage::ALL[usize::from(self.stage.load(Ordering::Relaxed)).min(Stage::ALL.len() - 1)];
        let f = f32::from_bits(self.fraction.load(Ordering::Relaxed));
        (stage, (!f.is_nan()).then_some(f))
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etapes_et_fraction() {
        let p = Progress::default();
        assert_eq!(p.get(), (Stage::Read, None));
        p.stage(Stage::Mesh);
        p.set(1.5);
        assert_eq!(p.get(), (Stage::Mesh, Some(1.0)));
        p.stage(Stage::Prepare);
        assert_eq!(p.get(), (Stage::Prepare, None));
        assert!(!p.cancelled());
        p.cancel();
        assert!(p.cancelled());
    }
}
