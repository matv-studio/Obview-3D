#version 460
#extension GL_GOOGLE_include_directive : require
#include "scene_common.glsl"

// Sélection au pixel : l'indice du triangle passe au fragment sans interpolation (gl_PrimitiveID exigerait
// la fonction « geometryShader »).
layout(location = 0) out flat uint v_triangle;
layout(location = 1) out vec3 v_world;

void main() {
    uint i = pc.indices.v[gl_VertexIndex];
    vec3 p = pc.positions.v[i] + pc.params.offsets[pc.attrs.v[i].y].xyz;
    v_triangle = uint(gl_VertexIndex) / 3u;
    v_world = p;
    gl_Position = pc.view_proj * vec4(p, 1.0);
}
