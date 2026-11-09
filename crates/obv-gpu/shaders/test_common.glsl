// Données d'essai lues directement par adresse (pas d'entrée de sommets fixe).
#extension GL_EXT_buffer_reference : require
#extension GL_EXT_scalar_block_layout : require

layout(buffer_reference, scalar, buffer_reference_align = 4) readonly buffer Vec3s { vec3 v[]; };
layout(buffer_reference, scalar, buffer_reference_align = 4) readonly buffer U32s { uint v[]; };

layout(push_constant, scalar) uniform Push {
    mat4 view_proj;
    Vec3s positions;
    Vec3s normals;
    U32s indices;
    uint index_count;
} pc;
