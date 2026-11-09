//! FBX (binaire et texte, toutes versions courantes) par ufbx : repère converti en Y vers le haut, unité le mètre,
//! normales générées si absentes. Chaque nœud portant un maillage devient une pièce (les instances sont
//! dupliquées avec leur transformation), découpée en plages par matériau. Matériaux lus en métal-rugosité
//! (ufbx convertit les modèles Phong / Lambert / Arnold / Standard Surface), textures intégrées ou voisines.

use std::{collections::HashMap, path::Path};

use crate::{Material, Mesh, Model, Part, ReadError, SubMesh, Texture};

pub fn read_fbx(path: &Path) -> Result<Model, ReadError> {
    let opts = ufbx::LoadOpts {
        target_axes: ufbx::CoordinateAxes::right_handed_y_up(),
        target_unit_meters: 1.0,
        space_conversion: ufbx::SpaceConversion::ModifyGeometry,
        generate_missing_normals: true,
        ..Default::default()
    };
    let path_str = path.to_str().ok_or_else(|| ReadError::Invalid("chemin non UTF-8".into()))?;
    let scene =
        ufbx::load_file(path_str, opts).map_err(|e| ReadError::Invalid(format!("FBX : {}", e.description.as_ref())))?;
    let dir = path.parent().unwrap_or(Path::new("."));

    // Textures : contenu intégré, sinon fichier (chemin absolu, puis relatif au FBX, puis simple nom).
    let mut textures: Vec<Texture> = Vec::new();
    let mut tex_slot: HashMap<u32, Option<u32>> = HashMap::new();
    let mut texture = |t: &ufbx::Texture, textures: &mut Vec<Texture>| -> Option<u32> {
        *tex_slot.entry(t.element.element_id).or_insert_with(|| {
            let data: Vec<u8> = if t.content.size > 0 {
                t.content.to_vec()
            } else {
                let candidates = [
                    std::path::PathBuf::from(t.absolute_filename.as_ref()),
                    dir.join(t.relative_filename.as_ref().replace('\\', "/")),
                    dir.join(Path::new(&t.filename.as_ref().replace('\\', "/")).file_name().unwrap_or_default()),
                ];
                candidates.iter().find_map(|p| std::fs::read(p).ok())?
            };
            let i = textures.len() as u32;
            let name = Path::new(&t.filename.as_ref().replace('\\', "/"))
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| t.element.name.to_string());
            textures.push(Texture { name, mime: Texture::sniff(&data).to_string(), data });
            Some(i)
        })
    };
    let mut materials: Vec<Material> = Vec::new();
    let mut mat_slot: HashMap<u32, u32> = HashMap::new();
    let mut material = |m: &ufbx::Material, materials: &mut Vec<Material>, textures: &mut Vec<Texture>| -> u32 {
        if let Some(i) = mat_slot.get(&m.element.element_id) {
            return *i;
        }
        let pbr = &m.pbr;
        let mut map_tex = |mm: &ufbx::MaterialMap, textures: &mut Vec<Texture>| {
            mm.texture.as_ref().and_then(|t| texture(t, textures))
        };
        let bc = pbr.base_color.value_vec4;
        let factor = if pbr.base_factor.has_value { pbr.base_factor.value_vec4.x as f32 } else { 1.0 };
        // Opacité : modèle PBR si présent, sinon transparence Phong (facteur × couleur moyenne).
        let fbx = &m.fbx;
        let opacity = if pbr.opacity.has_value && pbr.opacity.value_vec4.x < 0.999 {
            pbr.opacity.value_vec4.x as f32
        } else if fbx.transparency_factor.has_value {
            let tc = if fbx.transparency_color.has_value {
                let c = fbx.transparency_color.value_vec4;
                ((c.x + c.y + c.z) / 3.0) as f32
            } else {
                1.0
            };
            (1.0 - fbx.transparency_factor.value_vec4.x as f32 * tc).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let em = pbr.emission_color.value_vec4;
        let ef = if pbr.emission_factor.has_value { pbr.emission_factor.value_vec4.x as f32 } else { 1.0 };
        let out = Material {
            name: m.element.name.to_string(),
            base_color: [bc.x as f32 * factor, bc.y as f32 * factor, bc.z as f32 * factor, opacity],
            metallic: if pbr.metalness.has_value { pbr.metalness.value_vec4.x as f32 } else { 0.0 },
            roughness: if pbr.roughness.has_value { pbr.roughness.value_vec4.x as f32 } else { 0.5 },
            emissive: [em.x as f32 * ef, em.y as f32 * ef, em.z as f32 * ef],
            base_color_texture: map_tex(&pbr.base_color, textures),
            metallic_roughness_texture: match map_tex(&pbr.roughness, textures) {
                Some(t) => Some(t),
                None => map_tex(&pbr.metalness, textures),
            },
            normal_texture: map_tex(&pbr.normal_map, textures),
            occlusion_texture: map_tex(&pbr.ambient_occlusion, textures),
            emissive_texture: map_tex(&pbr.emission_color, textures),
            double_sided: false,
            blend: opacity < 0.999,
        };
        let i = materials.len() as u32;
        materials.push(out);
        mat_slot.insert(m.element.element_id, i);
        i
    };

    let mut parts = Vec::new();
    let mut tri = Vec::new();
    for node in scene.nodes.iter() {
        let Some(mesh) = node.mesh.as_ref() else { continue };
        if mesh.num_triangles == 0 {
            continue;
        }
        // Matériaux de l'instance (prioritaires) ou du maillage.
        let mats: Vec<Option<u32>> = if !node.materials.is_empty() {
            node.materials.iter().map(|m| Some(material(m, &mut materials, &mut textures))).collect()
        } else {
            mesh.materials.iter().map(|m| Some(material(m, &mut materials, &mut textures))).collect()
        };
        let to_world = node.geometry_to_world;
        let normal_m = ufbx::matrix_for_normals(&to_world);
        let (pos, nrm, uv) = (&mesh.vertex_position, &mesh.vertex_normal, &mesh.vertex_uv);
        // Triangles rangés par matériau.
        let mut by_mat: Vec<(Option<u32>, Vec<u32>)> = Vec::new();
        for (fi, face) in mesh.faces.iter().enumerate() {
            let m = mesh.face_material.get(fi).and_then(|&k| mats.get(k as usize).copied().flatten());
            let n = ufbx::triangulate_face_vec(&mut tri, mesh, *face);
            let slot = match by_mat.iter().position(|(k, _)| *k == m) {
                Some(s) => s,
                None => {
                    by_mat.push((m, Vec::new()));
                    by_mat.len() - 1
                }
            };
            by_mat[slot].1.extend_from_slice(&tri[..n as usize * 3]);
        }
        by_mat.sort_by_key(|(m, _)| m.map(|v| v as i64).unwrap_or(-1));
        let mut out = Mesh::default();
        let mut map: HashMap<(u32, u32, u32), u32> = HashMap::with_capacity(mesh.num_triangles * 2);
        for (m, corners) in &by_mat {
            let first = out.indices.len() as u32;
            for &corner in corners {
                let c = corner as usize;
                let pi = pos.indices[c];
                let ni = if nrm.exists { nrm.indices[c] } else { u32::MAX };
                let ti = if uv.exists { uv.indices[c] } else { u32::MAX };
                let idx = *map.entry((pi, ni, ti)).or_insert_with(|| {
                    let p = ufbx::transform_position(&to_world, pos.values[pi as usize]);
                    out.positions.push([p.x as f32, p.y as f32, p.z as f32]);
                    let nv = if nrm.exists {
                        let d = ufbx::transform_direction(&normal_m, nrm.values[ni as usize]);
                        let l = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt().max(1e-20);
                        [(d.x / l) as f32, (d.y / l) as f32, (d.z / l) as f32]
                    } else {
                        [0.0, 0.0, 0.0]
                    };
                    out.normals.push(nv);
                    if uv.exists {
                        let t = uv.values[ti as usize];
                        // Convention glTF : origine en haut à gauche de l'image (FBX : en bas).
                        out.uvs.push([t.x as f32, 1.0 - t.y as f32]);
                    }
                    (out.positions.len() - 1) as u32
                });
                out.indices.push(idx);
            }
            out.submeshes.push(SubMesh { first, count: out.indices.len() as u32 - first, material: *m });
        }
        if out.submeshes.iter().all(|s| s.material.is_none()) {
            out.submeshes.clear();
        }
        let name = node.element.name.to_string();
        let name = if name.is_empty() { format!("Pièce {}", parts.len() + 1) } else { name };
        parts.push(Part { name, mesh: out });
    }
    Ok(Model { parts, unit_m: 1.0, materials, textures })
}
