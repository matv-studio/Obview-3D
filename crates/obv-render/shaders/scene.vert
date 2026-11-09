#version 460
#extension GL_GOOGLE_include_directive : require
#include "scene_common.glsl"

layout(location = 0) out vec3 v_normal;
layout(location = 1) out vec3 v_world;
layout(location = 2) out flat uint v_color;

void main() {
    uint i = pc.indices.v[gl_VertexIndex];
    vec3 p = pc.positions.v[i];
    v_normal = pc.normals.v[i];
    v_world = p;
    v_color = pc.colors.v[i];
    gl_Position = pc.view_proj * vec4(p, 1.0);
}
