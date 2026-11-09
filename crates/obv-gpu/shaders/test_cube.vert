#version 460
#extension GL_GOOGLE_include_directive : require
#include "test_common.glsl"

layout(location = 0) out vec3 out_normal;

void main() {
    uint i = pc.indices.v[gl_VertexIndex];
    out_normal = pc.normals.v[i];
    gl_Position = pc.view_proj * vec4(pc.positions.v[i], 1.0);
}
