#version 460
layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 out_color;
#ifdef OBV_GL
layout(origin_upper_left) in vec4 gl_FragCoord;
layout(std140, binding = 0) uniform Push {
#else
layout(push_constant) uniform Push {
#endif
    vec4 top;
    vec4 bottom;
    // Bande gardée : band.z <= dot(gl_FragCoord.xy, band.xy) < band.w.
    vec4 band;
} pc;

void main() {
    float u = dot(gl_FragCoord.xy, pc.band.xy);
    if (u < pc.band.z || u >= pc.band.w) discard;
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
