#version 460
#extension GL_GOOGLE_include_directive : require
#include "scene_common.glsl"

layout(location = 0) in vec3 v_normal;
layout(location = 1) in vec3 v_world;
layout(location = 2) in flat uint v_color;
layout(location = 0) out vec4 out_color;

vec3 srgb_to_linear(vec3 c) {
    return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(0.04045, c));
}

// Éclairage de présentation de pièces : ciel/sol, lumière principale, lumière d'appoint, contre-jour, reflet.
void main() {
    vec3 base = srgb_to_linear(unpackUnorm4x8(v_color).rgb);
    vec3 n = normalize(v_normal);
    vec3 v = normalize(pc.eye.xyz - v_world);
    if (!gl_FrontFacing) n = -n;
    // Repère de la caméra : les lumières suivent la vue, comme dans les visionneuses CAO.
    vec3 fwd = -v;
    vec3 right = normalize(cross(fwd, vec3(0.0, 1.0, 0.0) + vec3(1e-4, 0.0, 0.0)));
    vec3 up = cross(right, fwd);
    vec3 key = normalize(-fwd * 0.6 + up * 0.65 - right * 0.45);
    vec3 fill = normalize(-fwd * 0.8 - up * 0.2 + right * 0.6);
    vec3 rim = normalize(fwd * 0.7 + up * 0.5);

    float hemi = 0.5 + 0.5 * dot(n, vec3(0.0, 1.0, 0.0));
    vec3 ambient = mix(vec3(0.20, 0.19, 0.18), vec3(0.42, 0.44, 0.47), hemi);
    float d_key = max(dot(n, key), 0.0);
    float d_fill = max(dot(n, fill), 0.0);
    float d_rim = pow(max(dot(n, rim), 0.0), 2.0);
    vec3 h = normalize(key + v);
    float shininess = 48.0;
    float spec = pow(max(dot(n, h), 0.0), shininess) * (shininess + 8.0) / 25.13;
    float fresnel = 0.04 + 0.96 * pow(1.0 - max(dot(n, v), 0.0), 5.0);
    vec3 color = base * (ambient * 0.55 + d_key * 0.85 + d_fill * 0.28)
               + vec3(1.0) * (spec * 0.12 * (0.3 + 0.7 * fresnel))
               + base * d_rim * 0.18
               + vec3(fresnel * 0.06) * hemi;
    out_color = vec4(color, 1.0);
}
