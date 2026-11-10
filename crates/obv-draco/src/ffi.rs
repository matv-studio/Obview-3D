//! Pont `cxx` vers `cpp/codec.cpp`.

#[cxx::bridge(namespace = "obv_draco")]
pub mod bridge {
    /// Maillage encodé : flux Draco, nombre de points et de faces après encodage (comptes des accesseurs glTF),
    /// identifiants uniques des attributs (-1 : absent), message d'erreur (vide : succès).
    struct Encoded {
        data: Vec<u8>,
        points: u32,
        faces: u32,
        position_id: i32,
        normal_id: i32,
        uv_id: i32,
        error: String,
    }

    struct EncodeOptions {
        position_bits: i32,
        normal_bits: i32,
        uv_bits: i32,
        encode_speed: i32,
        decode_speed: i32,
    }

    unsafe extern "C++" {
        include!("codec.h");

        /// `positions` et `normals` : 3 flottants par sommet (`normals` vide : absentes) ; `uvs` : 2 par sommet ;
        /// `indices` : 3 par triangle.
        fn encode(positions: &[f32], normals: &[f32], uvs: &[f32], indices: &[u32], options: &EncodeOptions)
        -> Encoded;

        type Decoded;
        /// Décode un flux Draco (maillage) ; erreur dans `decode_error`.
        fn decode(data: &[u8]) -> UniquePtr<Decoded>;
        fn decode_error(self: &Decoded) -> String;
        fn num_points(self: &Decoded) -> u32;
        fn indices(self: &Decoded) -> Vec<u32>;
        /// Valeurs de l'attribut d'identifiant unique `id`, converties en flottants (`components` par point,
        /// entiers normalisés ramenés à [0, 1] ou [-1, 1]) ; vide s'il n'existe pas.
        fn attribute(self: &Decoded, id: u32, components: u32) -> Vec<f32>;
    }
}
