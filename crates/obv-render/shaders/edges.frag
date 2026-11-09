#version 460
#extension GL_EXT_buffer_reference : require
#extension GL_EXT_scalar_block_layout : require

layout(buffer_reference, scalar, buffer_reference_align = 4) readonly buffer Vec3s { vec3 v[]; };

// 128 octets : limite garantie par Vulkan pour les constantes poussées.
layout(push_constant, scalar) uniform Push {
    mat4 view_proj;
    // Par arête : extrémité a, extrémité b, normales des deux faces (nulles : arête vive).
    Vec3s edges;
    vec2 viewport;
    float width;
    uint color;
    vec3 eye;
    float pad;
    vec4 band;
} pc;

layout(location = 0) in float v_side;
layout(location = 0) out vec4 out_color;

void main() {
    float u = dot(gl_FragCoord.xy, pc.band.xy);
    if (u < pc.band.z || u >= pc.band.w) discard;
    // Bord adouci sur un pixel.
    float a = clamp(pc.width * 0.5 + 0.5 - abs(v_side), 0.0, 1.0);
    vec4 c = unpackUnorm4x8(pc.color);
    out_color = vec4(c.rgb * c.a, c.a) * a;
}
