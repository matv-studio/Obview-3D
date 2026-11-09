//! FBX (binaire et texte, toutes versions courantes) par ufbx : repère converti en Y vers le haut, unité le mètre,
//! normales générées si absentes. Chaque nœud portant un maillage devient une pièce (les instances sont
//! dupliquées avec leur transformation).

use std::{collections::HashMap, path::Path};

use crate::{Mesh, Model, Part, ReadError};

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
    let mut parts = Vec::new();
    let mut tri = Vec::new();
    for node in scene.nodes.iter() {
        let Some(mesh) = node.mesh.as_ref() else { continue };
        if mesh.num_triangles == 0 {
            continue;
        }
        let to_world = node.geometry_to_world;
        let normal_m = ufbx::matrix_for_normals(&to_world);
        let pos = &mesh.vertex_position;
        let nrm = &mesh.vertex_normal;
        let mut out = Mesh::default();
        let mut map: HashMap<(u32, u32), u32> = HashMap::with_capacity(mesh.num_triangles * 2);
        for face in mesh.faces.iter() {
            let n = ufbx::triangulate_face_vec(&mut tri, mesh, *face);
            for &corner in &tri[..n as usize * 3] {
                let pi = pos.indices[corner as usize];
                let ni = if nrm.exists { nrm.indices[corner as usize] } else { u32::MAX };
                let idx = *map.entry((pi, ni)).or_insert_with(|| {
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
                    (out.positions.len() - 1) as u32
                });
                out.indices.push(idx);
            }
        }
        if !nrm.exists {
            out = crate::normals::smooth(&out.positions, &out.indices, crate::stl::CREASE_DEG);
        }
        let name = node.element.name.to_string();
        let name = if name.is_empty() { format!("Pièce {}", parts.len() + 1) } else { name };
        parts.push(Part { name, mesh: out });
    }
    Ok(Model { parts, unit_m: 1.0 })
}
