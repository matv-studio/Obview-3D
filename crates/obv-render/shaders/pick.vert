#version 460
#extension GL_GOOGLE_include_directive : require
#include "scene_common.glsl"

// Sélection au pixel : l'indice du triangle passe au fragment sans interpolation (gl_PrimitiveID exigerait
// la fonction « geometryShader »).
layout(location = 0) out flat uint v_triangle;

void main() {
    uint i = pc.indices.v[gl_VertexIndex];
    v_triangle = uint(gl_VertexIndex) / 3u;
    gl_Position = pc.view_proj * vec4(pc.positions.v[i], 1.0);
}
