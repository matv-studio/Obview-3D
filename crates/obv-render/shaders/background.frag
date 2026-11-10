#version 460
#extension GL_GOOGLE_include_directive : require
#include "light_block.glsl"
layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec2 v_ndc;
layout(location = 0) out vec4 out_color;
#ifdef OBV_GL
layout(origin_upper_left) in vec4 gl_FragCoord;
layout(std430, binding = 4) readonly buffer ParamsBuf {
    vec3 eye;
    uint ortho;
    vec3 forward;
    uint flags;
    vec4 plane;
    LIGHT_BLOCK
} params_b;
layout(std140, binding = 0) uniform Push {
    vec4 top;
    vec4 bottom;
    // Bande gardée : band.z <= dot(gl_FragCoord.xy, band.xy) < band.w.
    vec4 band;
    // Inverse de la projection en perspective de la vue (directions du fond).
    mat4 inv_view_proj;
} pc;
layout(binding = 1) uniform sampler2D env_map;
#define LIGHT params_b
#else
#extension GL_EXT_buffer_reference : require
#extension GL_EXT_scalar_block_layout : require
layout(buffer_reference, scalar, buffer_reference_align = 4) readonly buffer Params {
    vec3 eye;
    uint ortho;
    vec3 forward;
    uint flags;
    vec4 plane;
    LIGHT_BLOCK
};
layout(push_constant, scalar) uniform Push {
    vec4 top;
    vec4 bottom;
    vec4 band;
    mat4 inv_view_proj;
    Params params;
} pc;
layout(set = 0, binding = 0) uniform sampler2D env_map;
#define LIGHT pc.params
#endif
#include "light.glsl"

void main() {
    float u = dot(gl_FragCoord.xy, pc.band.xy);
    if (u < pc.band.z || u >= pc.band.w) discard;
    // Fond d'environnement si réglé et permis par l'appelant (alpha du haut : bandes des rendus exclues).
    if (LIGHT.light_bg.x > 0.5 && pc.top.a > 0.5) {
        // Environnement : direction du rayon de vue (profondeur inversée : 1 au plan proche).
        vec4 a = pc.inv_view_proj * vec4(v_ndc, 1.0, 1.0);
        vec4 b = pc.inv_view_proj * vec4(v_ndc, 0.5, 1.0);
        vec3 d = normalize(b.xyz / b.w - a.xyz / a.w);
        out_color = vec4(env_background(d), 1.0);
        return;
    }
    // v_uv.y = 0 en bas (fenêtre retournée), 1 en haut.
    float t = clamp(v_uv.y, 0.0, 1.0);
    vec3 c = mix(pc.bottom.rgb, pc.top.rgb, smoothstep(0.0, 1.0, t));
    // Léger assombrissement des bords, pour concentrer le regard sur la pièce.
    vec2 d = v_uv - 0.5;
    c *= 1.0 - 0.10 * dot(d, d) * 2.0;
    // Tramage léger contre les bandes du dégradé sur 8 bits.
    float n = fract(sin(dot(gl_FragCoord.xy, vec2(12.9898, 78.233))) * 43758.5453);
    c += (n - 0.5) / 255.0 * 0.6;
    out_color = vec4(c, 1.0);
}
