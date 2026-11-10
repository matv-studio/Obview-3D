#ifdef OBV_GL
// OpenGL 4.30 (D-30) : tampons liés ; arêtes lues en flottants (pas de 12 octets).
layout(std430, binding = 0) readonly buffer Edges { float edges_f[]; };
layout(std430, binding = 1) readonly buffer Parts { uint parts_u[]; };
layout(std430, binding = 4) readonly buffer ParamsBuf {
    vec3 eye;
    uint ortho;
    vec3 forward;
    uint flags;
    vec4 plane;
    // Bloc d'éclairage (20 vec4, voir scene_common.glsl), non lu ici.
    vec4 light_unused[20];
    vec4 offsets[];
} params_b;
layout(std140, binding = 0) uniform Push {
    mat4 view_proj;
    vec2 viewport;
    float width;
    uint color;
    vec4 band;
} pc;
#define EDGE(i) vec3(edges_f[3u * (i)], edges_f[3u * (i) + 1u], edges_f[3u * (i) + 2u])
#define PART(i) parts_u[i]
#define PARAMS params_b
#define VERTEX_ID gl_VertexID
#ifdef OBV_STAGE_FRAG
layout(origin_upper_left) in vec4 gl_FragCoord;
#endif
#else
#extension GL_EXT_buffer_reference : require
#extension GL_EXT_scalar_block_layout : require

layout(buffer_reference, scalar, buffer_reference_align = 4) readonly buffer Vec3s { vec3 v[]; };
layout(buffer_reference, scalar, buffer_reference_align = 4) readonly buffer U32s { uint v[]; };
layout(buffer_reference, scalar, buffer_reference_align = 4) readonly buffer Params {
    vec3 eye;
    uint ortho;
    vec3 forward;
    uint flags;
    vec4 plane;
    // Bloc d'éclairage (20 vec4, voir scene_common.glsl), non lu ici.
    vec4 light_unused[20];
    vec4 offsets[];
};

// 128 octets : limite garantie par Vulkan pour les constantes poussées.
layout(push_constant, scalar) uniform Push {
    mat4 view_proj;
    // Par arête : extrémité a, extrémité b, normales des deux faces (nulles : arête vive).
    Vec3s edges;
    // Pièce de chaque arête (décalage de la vue éclatée).
    U32s parts;
    Params params;
    vec2 viewport;
    float width;
    uint color;
    vec4 band;
} pc;
#define EDGE(i) pc.edges.v[i]
#define PART(i) pc.parts.v[i]
#define PARAMS pc.params
#define VERTEX_ID gl_VertexIndex
#endif

vec4 clip_position(vec4 p) {
#ifdef OBV_GL
    // OBV_DEPTH_REMAP est défini à l'exécution (1.0 sans glClipControl, 0.0 sinon) : il reste tel quel au
    // prétraitement de la construction.
    if (OBV_DEPTH_REMAP > 0.5) p.z = 2.0 * p.z - p.w;
#endif
    return p;
}
