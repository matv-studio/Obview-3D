#version 460
#extension GL_GOOGLE_include_directive : require
#include "scene_common.glsl"

layout(location = 0) in vec3 v_normal;
layout(location = 1) in vec3 v_world;
layout(location = 2) in flat uint v_color;
layout(location = 3) in vec3 v_bary;
layout(location = 0) out vec4 out_color;

// Rayons X : chaque surface ajoute une lueur d'autant plus forte qu'elle est vue de biais (somme sans tri).
void main() {
    if (outside_band(gl_FragCoord.xy) || cut_away(v_world)) discard;
    vec3 n = normalize(v_normal);
    vec3 v = to_eye(v_world);
    float f = 1.0 - abs(dot(n, v));
    float a = 0.035 + 0.30 * f * f;
    out_color = vec4(vec3(0.55, 0.78, 1.0) * a, 0.0);
}
