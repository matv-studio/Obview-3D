#extension GL_EXT_buffer_reference : require
#extension GL_EXT_scalar_block_layout : require

layout(buffer_reference, scalar, buffer_reference_align = 4) readonly buffer Vec3s { vec3 v[]; };
layout(buffer_reference, scalar, buffer_reference_align = 4) readonly buffer U32s { uint v[]; };

// 128 octets : limite garantie par Vulkan pour les constantes poussées.
layout(push_constant, scalar) uniform Push {
    mat4 view_proj;
    vec3 eye;
    // Rendu : 0 éclairé, 1 couleur, 2 normales, 3 topologie, 4 rayons X, 5 arêtes (surfaces).
    uint mode;
    Vec3s positions;
    Vec3s normals;
    U32s indices;
    U32s colors;
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
