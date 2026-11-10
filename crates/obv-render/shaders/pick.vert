#version 460
#extension GL_GOOGLE_include_directive : require
#include "scene_common.glsl"

// Sélection au pixel : l'indice du triangle passe au fragment sans interpolation (gl_PrimitiveID exigerait
// la fonction « geometryShader »).
layout(location = 0) out flat uint v_triangle;
layout(location = 1) out vec3 v_world;

void main() {
    uint i = INDEX(VERTEX_ID);
    vec3 p = POSITION(i) + PARAMS.offsets[ATTR(i).y].xyz;
    v_triangle = uint(VERTEX_ID) / 3u;
    v_world = p;
    gl_Position = clip_position(pc.view_proj * vec4(p, 1.0));
}
