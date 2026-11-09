#version 460
#extension GL_GOOGLE_include_directive : require
#include "edges_common.glsl"

layout(location = 0) in float v_side;
layout(location = 1) in vec3 v_world;
layout(location = 0) out vec4 out_color;

void main() {
    float u = dot(gl_FragCoord.xy, pc.band.xy);
    if (u < pc.band.z || u >= pc.band.w) discard;
    if ((pc.params.flags & 1u) != 0u && dot(pc.params.plane.xyz, v_world) > pc.params.plane.w) discard;
    // Bord adouci sur un pixel.
    float a = clamp(pc.width * 0.5 + 0.5 - abs(v_side), 0.0, 1.0);
    vec4 c = unpackUnorm4x8(pc.color);
    out_color = vec4(c.rgb * c.a, c.a) * a;
}
