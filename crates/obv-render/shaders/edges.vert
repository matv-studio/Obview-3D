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

layout(location = 0) out float v_side;

// Arêtes vives : chaque segment devient un rectangle de largeur constante à l'écran (6 sommets).
void main() {
    uint seg = uint(gl_VertexIndex) / 6u;
    uint corner = uint(gl_VertexIndex) % 6u;
    vec3 pa = pc.edges.v[seg * 4u];
    vec3 pb = pc.edges.v[seg * 4u + 1u];
    vec3 n0 = pc.edges.v[seg * 4u + 2u];
    vec3 n1 = pc.edges.v[seg * 4u + 3u];
    // Arête douce : tracée seulement sur le contour apparent (une face tournée vers l'œil, l'autre non).
    bool smooth_edge = dot(n0, n0) > 0.0;
    vec3 view = pc.eye - (pa + pb) * 0.5;
    bool silhouette = (dot(n0, view) > 0.0) != (dot(n1, view) > 0.0);
    vec4 a = pc.view_proj * vec4(pa, 1.0);
    vec4 b = pc.view_proj * vec4(pb, 1.0);
    // Segment derrière l'œil ou arête douce hors contour : rien à tracer.
    if ((a.w <= 0.0 && b.w <= 0.0) || (smooth_edge && !silhouette)) {
        gl_Position = vec4(0.0, 0.0, -2.0, 1.0);
        v_side = 0.0;
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
    // Léger décalage vers l'œil (profondeur inversée) : l'arête passe devant sa face.
    p.z *= 1.0004;
    v_side = side * (pc.width * 0.5 + 1.0);
    gl_Position = p;
}
