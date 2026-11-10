#version 460
#extension GL_GOOGLE_include_directive : require
#include "scene_common.glsl"

layout(location = 0) out vec3 v_normal;
layout(location = 1) out vec3 v_world;
layout(location = 2) out flat uint v_color;
// Coordonnées barycentriques (fil de fer de la topologie).
layout(location = 3) out vec3 v_bary;

void main() {
    uint i = INDEX(VERTEX_ID);
    uvec2 a = ATTR(i);
    vec3 p = POSITION(i) + PARAMS.offsets[a.y].xyz;
    v_normal = NORMAL(i);
    v_world = p;
    v_color = a.x;
    uint corner = uint(VERTEX_ID) % 3u;
    v_bary = vec3(corner == 0u, corner == 1u, corner == 2u);
    gl_Position = clip_position(pc.view_proj * vec4(p, 1.0));
}
