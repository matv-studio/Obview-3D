#extension GL_EXT_buffer_reference : require
#extension GL_EXT_scalar_block_layout : require

layout(buffer_reference, scalar, buffer_reference_align = 4) readonly buffer Vec3s { vec3 v[]; };
layout(buffer_reference, scalar, buffer_reference_align = 4) readonly buffer U32s { uint v[]; };
layout(buffer_reference, scalar, buffer_reference_align = 4) readonly buffer UVec2s { uvec2 v[]; };

// Paramètres de l'image, communs à tous les tracés : œil, projection, coupe, éclatement.
layout(buffer_reference, scalar, buffer_reference_align = 4) readonly buffer Params {
    vec3 eye;
    // 1 : projection orthographique (rayons parallèles à `forward`).
    uint ortho;
    vec3 forward;
    // Bit 0 : coupe active.
    uint flags;
    // Plan de coupe : on garde dot(plane.xyz, p) <= plane.w.
    vec4 plane;
    // Décalage de chaque pièce (vue éclatée), dans l'ordre du tracé.
    vec4 offsets[];
};

// 128 octets : limite garantie par Vulkan pour les constantes poussées.
layout(push_constant, scalar) uniform Push {
    mat4 view_proj;
    Vec3s positions;
    Vec3s normals;
    U32s indices;
    // Par sommet : couleur sRGB 8 bits, indice de la pièce.
    UVec2s attrs;
    Params params;
    // Rendu : 0 éclairé, 1 couleur, 2 normales, 3 topologie, 4 rayons X, 5 arêtes (surfaces).
    uint mode;
    uint pad;
    // Bande de l'inspecteur : fragments gardés si band.z <= dot(gl_FragCoord.xy, band.xy) < band.w.
    vec4 band;
} pc;

const uint MODE_LIT = 0u;
const uint MODE_COLOR = 1u;
const uint MODE_NORMALS = 2u;
const uint MODE_TOPOLOGY = 3u;
const uint MODE_XRAY = 4u;
const uint MODE_EDGES = 5u;

// `pixel` : gl_FragCoord.xy (la fonction sert aussi aux nuanceurs de sommets qui incluent ce fichier).
bool outside_band(vec2 pixel) {
    float u = dot(pixel, pc.band.xy);
    return u < pc.band.z || u >= pc.band.w;
}

bool section_on() {
    return (pc.params.flags & 1u) != 0u;
}

// Point du monde au-delà du plan de coupe (partie retirée).
bool cut_away(vec3 p) {
    return section_on() && dot(pc.params.plane.xyz, p) > pc.params.plane.w;
}

// Direction du rayon de vue passant par `p` (de l'œil vers le point).
vec3 view_ray(vec3 p) {
    return pc.params.ortho != 0u ? pc.params.forward : p - pc.params.eye;
}

// Direction vers l'œil, unitaire.
vec3 to_eye(vec3 p) {
    return pc.params.ortho != 0u ? -pc.params.forward : normalize(pc.params.eye - p);
}
