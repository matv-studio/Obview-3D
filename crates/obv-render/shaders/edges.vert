#version 460
#extension GL_GOOGLE_include_directive : require

#include "edges_common.glsl"

layout(location = 0) out float v_side;
layout(location = 1) out vec3 v_world;

// Arêtes vives : chaque segment devient un rectangle de largeur constante à l'écran (6 sommets).
void main() {
    uint seg = uint(VERTEX_ID) / 6u;
    uint corner = uint(VERTEX_ID) % 6u;
    vec3 shift = PARAMS.offsets[PART(seg)].xyz;
    vec3 pa = EDGE(seg * 4u) + shift;
    vec3 pb = EDGE(seg * 4u + 1u) + shift;
    vec3 n0 = EDGE(seg * 4u + 2u);
    vec3 n1 = EDGE(seg * 4u + 3u);
    // Arête douce : tracée seulement sur le contour apparent (une face tournée vers l'œil, l'autre non).
    bool smooth_edge = dot(n0, n0) > 0.0;
    vec3 view = PARAMS.ortho != 0u ? -PARAMS.forward : PARAMS.eye - (pa + pb) * 0.5;
    bool silhouette = (dot(n0, view) > 0.0) != (dot(n1, view) > 0.0);
    vec4 a = pc.view_proj * vec4(pa, 1.0);
    vec4 b = pc.view_proj * vec4(pb, 1.0);
    // Segment derrière l'œil ou arête douce hors contour : rien à tracer.
    if ((a.w <= 0.0 && b.w <= 0.0) || (smooth_edge && !silhouette)) {
        gl_Position = vec4(0.0, 0.0, -2.0, 1.0);
        v_side = 0.0;
        v_world = pa;
        return;
    }
    // Découpe au plan proche approximée : on ramène l'extrémité derrière l'œil juste devant.
    if (a.w <= 1e-6) a = mix(b, a, (b.w - 1e-6) / (b.w - a.w));
    if (b.w <= 1e-6) b = mix(a, b, (a.w - 1e-6) / (a.w - b.w));
    vec2 sa = a.xy / a.w * pc.viewport * 0.5;
    vec2 sb = b.xy / b.w * pc.viewport * 0.5;
    vec2 dir = sb - sa;
    float len = length(dir);
    dir = len > 1e-6 ? dir / len : vec2(1.0, 0.0);
    vec2 nrm = vec2(-dir.y, dir.x);
    bool end = corner == 1u || corner == 2u || corner == 4u;
    float side = (corner == 0u || corner == 1u || corner == 3u) ? -1.0 : 1.0;
    vec4 p = end ? b : a;
    vec2 off = (nrm * side * (pc.width * 0.5 + 1.0) + dir * (end ? 1.0 : -1.0) * 0.5) / (pc.viewport * 0.5);
    p.xy += off * p.w;
    // Léger décalage vers l'œil (profondeur inversée) : l'arête passe devant sa face. En perspective, la
    // profondeur est proportionnelle à l'inverse de la distance ; en orthographique, linéaire.
    if (PARAMS.ortho != 0u) p.z += 4e-6 * p.w; else p.z *= 1.0004;
    v_world = end ? pb : pa;
    v_side = side * (pc.width * 0.5 + 1.0);
    gl_Position = clip_position(p);
}
