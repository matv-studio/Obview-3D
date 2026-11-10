#ifdef OBV_GL
// OpenGL 4.30 (moteur de secours, D-30) : tampons de stockage liés à des points fixes au lieu des adresses ;
// positions et normales lues en flottants (pas de 12 octets, comme la disposition scalaire de Vulkan).
layout(std430, binding = 0) readonly buffer Positions { float positions_f[]; };
layout(std430, binding = 1) readonly buffer Normals { float normals_f[]; };
layout(std430, binding = 2) readonly buffer Indices { uint indices_u[]; };
layout(std430, binding = 3) readonly buffer Attrs { uvec2 attrs_u[]; };
layout(std430, binding = 4) readonly buffer ParamsBuf {
    vec3 eye;
    uint ortho;
    vec3 forward;
    uint flags;
    vec4 plane;
    vec4 offsets[];
} params_b;
layout(std140, binding = 0) uniform Push {
    mat4 view_proj;
    uint mode;
    uint pad0;
    uint pad1;
    uint pad2;
    vec4 band;
} pc;
#define POSITION(i) vec3(positions_f[3u * (i)], positions_f[3u * (i) + 1u], positions_f[3u * (i) + 2u])
#define NORMAL(i) vec3(normals_f[3u * (i)], normals_f[3u * (i) + 1u], normals_f[3u * (i) + 2u])
#define INDEX(i) indices_u[i]
#define ATTR(i) attrs_u[i]
#define PARAMS params_b
#define VERTEX_ID gl_VertexID
#ifdef OBV_STAGE_FRAG
// Pixels comptés depuis le haut, comme sous Vulkan (bandes de l'inspecteur, hachures).
layout(origin_upper_left) in vec4 gl_FragCoord;
#endif
#else
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
#define POSITION(i) pc.positions.v[i]
#define NORMAL(i) pc.normals.v[i]
#define INDEX(i) pc.indices.v[i]
#define ATTR(i) pc.attrs.v[i]
#define PARAMS pc.params
#define VERTEX_ID gl_VertexIndex
#endif

// Position de découpage finale. OpenGL sans glClipControl (profondeur de -1 à 1) : profondeur ramenée à la
// convention de Vulkan (0 à 1).
vec4 clip_position(vec4 p) {
#ifdef OBV_GL
    // OBV_DEPTH_REMAP est défini à l'exécution (1.0 sans glClipControl, 0.0 sinon) : il reste tel quel au
    // prétraitement de la construction.
    if (OBV_DEPTH_REMAP > 0.5) p.z = 2.0 * p.z - p.w;
#endif
    return p;
}

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
    return (PARAMS.flags & 1u) != 0u;
}

// Point du monde au-delà du plan de coupe (partie retirée).
bool cut_away(vec3 p) {
    return section_on() && dot(PARAMS.plane.xyz, p) > PARAMS.plane.w;
}

// Direction du rayon de vue passant par `p` (de l'œil vers le point).
vec3 view_ray(vec3 p) {
    return PARAMS.ortho != 0u ? PARAMS.forward : p - PARAMS.eye;
}

// Direction vers l'œil, unitaire.
vec3 to_eye(vec3 p) {
    return PARAMS.ortho != 0u ? -PARAMS.forward : normalize(PARAMS.eye - p);
}
