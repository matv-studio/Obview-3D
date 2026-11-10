//! Compression Draco (`KHR_draco_mesh_compression`) par la bibliothèque de référence de Google (D-30).
//!
//! C'est le codec que Blender, three.js, glTFast (Unity) et Babylon.js embarquent : un flux qu'il encode est lu
//! par tous. Sans `DRACO_DIR` à la construction, [`available`] est faux et les fonctions rendent une erreur.

#[cfg(draco)]
mod ffi;

/// Réglages d'encodage : bits de quantification par attribut, vitesses (0 : meilleure compression, 10 : plus
/// rapide).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncodeOptions {
    pub position_bits: i32,
    pub normal_bits: i32,
    pub uv_bits: i32,
    pub encode_speed: i32,
    pub decode_speed: i32,
}

impl Default for EncodeOptions {
    /// Normales sur 10 bits et coordonnées de texture sur 12 bits (valeurs par défaut de Blender et de
    /// glTF-Transform) ; vitesses 3 et 5 : Edgebreaker et prédictions complètes, décodage rapide.
    fn default() -> Self {
        EncodeOptions { position_bits: 16, normal_bits: 10, uv_bits: 12, encode_speed: 3, decode_speed: 5 }
    }
}

/// Primitive encodée : flux, comptes des accesseurs glTF, identifiants uniques des attributs.
#[derive(Clone, Debug)]
pub struct Encoded {
    pub data: Vec<u8>,
    pub points: u32,
    pub faces: u32,
    pub position_id: u32,
    pub normal_id: Option<u32>,
    pub uv_id: Option<u32>,
}

/// Draco est-il présent dans cette construction ?
pub const fn available() -> bool {
    cfg!(draco)
}

#[cfg(not(draco))]
const ABSENT: &str = "compression Draco absente de cette construction";

/// Encode un maillage indexé ; `normals` et `uvs` vides : absents.
pub fn encode(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    uvs: &[[f32; 2]],
    indices: &[u32],
    options: EncodeOptions,
) -> Result<Encoded, String> {
    #[cfg(draco)]
    {
        let r = ffi::bridge::encode(
            positions.as_flattened(),
            normals.as_flattened(),
            uvs.as_flattened(),
            indices,
            &ffi::bridge::EncodeOptions {
                position_bits: options.position_bits,
                normal_bits: options.normal_bits,
                uv_bits: options.uv_bits,
                encode_speed: options.encode_speed,
                decode_speed: options.decode_speed,
            },
        );
        if !r.error.is_empty() {
            return Err(format!("Draco : {}", r.error));
        }
        let id = |v: i32| u32::try_from(v).ok();
        Ok(Encoded {
            data: r.data,
            points: r.points,
            faces: r.faces,
            position_id: id(r.position_id).ok_or("Draco : positions absentes")?,
            normal_id: id(r.normal_id),
            uv_id: id(r.uv_id),
        })
    }
    #[cfg(not(draco))]
    {
        let _ = (positions, normals, uvs, indices, options);
        Err(ABSENT.into())
    }
}

/// Maillage décodé.
pub struct Decoded {
    #[cfg(draco)]
    inner: cxx::UniquePtr<ffi::bridge::Decoded>,
}

impl Decoded {
    pub fn points(&self) -> u32 {
        #[cfg(draco)]
        return self.inner.num_points();
        #[cfg(not(draco))]
        0
    }

    pub fn indices(&self) -> Vec<u32> {
        #[cfg(draco)]
        return self.inner.indices();
        #[cfg(not(draco))]
        Vec::new()
    }

    /// Attribut d'identifiant unique `id` en flottants (`components` par point) ; `None` s'il est absent.
    pub fn attribute(&self, id: u32, components: u32) -> Option<Vec<f32>> {
        #[cfg(draco)]
        {
            let v = self.inner.attribute(id, components);
            (!v.is_empty()).then_some(v)
        }
        #[cfg(not(draco))]
        {
            let _ = (id, components);
            None
        }
    }
}

/// Décode un flux Draco (maillage).
pub fn decode(data: &[u8]) -> Result<Decoded, String> {
    #[cfg(draco)]
    {
        let inner = ffi::bridge::decode(data);
        let e = inner.decode_error();
        if !e.is_empty() {
            return Err(format!("Draco : {e}"));
        }
        Ok(Decoded { inner })
    }
    #[cfg(not(draco))]
    {
        let _ = data;
        Err(ABSENT.into())
    }
}

#[cfg(all(test, draco))]
mod tests {
    use super::*;

    /// Cube : 24 sommets (normales par face), 12 triangles ; aller-retour par le décodeur de référence.
    #[test]
    fn aller_retour() {
        let mut pos = Vec::new();
        let mut nrm = Vec::new();
        let mut uv = Vec::new();
        let mut idx = Vec::new();
        for axis in 0..3 {
            for sign in [-1.0f32, 1.0] {
                let base = pos.len() as u32;
                let mut n = [0.0; 3];
                n[axis] = sign;
                let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                    let mut p = [0.0f32; 3];
                    p[axis] = sign * 0.05;
                    p[u] = a * 0.05;
                    p[v] = b * 0.05;
                    pos.push(p);
                    nrm.push(n);
                    uv.push([(a + 1.0) * 0.5, (b + 1.0) * 0.5]);
                }
                idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        let e = encode(&pos, &nrm, &uv, &idx, EncodeOptions::default()).unwrap();
        assert_eq!(e.faces, 12);
        assert!(e.points >= 8 && e.normal_id.is_some() && e.uv_id.is_some());
        let d = decode(&e.data).unwrap();
        assert_eq!(d.points(), e.points);
        assert_eq!(d.indices().len(), 36);
        let p = d.attribute(e.position_id, 3).unwrap();
        // 16 bits sur 0,1 : pas de 1,5 µm.
        for v in p {
            assert!((v.abs() - 0.05).abs() < 2e-6, "{v}");
        }
        let n = d.attribute(e.normal_id.unwrap(), 3).unwrap();
        for c in n.chunks(3) {
            assert!(((c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() - 1.0).abs() < 0.01);
        }
        assert!(decode(&[1, 2, 3]).is_err());
    }
}
