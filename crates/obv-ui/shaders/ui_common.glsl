// Primitives de l'interface (voir paint.rs : Prim).
#ifndef OBV_GL
#extension GL_EXT_buffer_reference : require
#extension GL_EXT_scalar_block_layout : require
#endif

struct Prim {
    vec4 rect;
    vec4 clip;
    vec4 radii;
    vec4 color0;
    vec4 color1;
    vec4 border;
    vec4 uv;
    vec4 params;
};

#ifdef OBV_GL
// OpenGL 4.30 (D-30) : primitives dans un tampon lié, taille de l'écran en bloc uniforme.
layout(std430, binding = 0) readonly buffer Prims { Prim p[]; } prims_b;
layout(std140, binding = 0) uniform Push { vec2 screen; } pc;
#define PRIM(i) prims_b.p[i]
#define VERTEX_ID gl_VertexID
#define INSTANCE_ID gl_InstanceID
#else
layout(buffer_reference, scalar, buffer_reference_align = 16) readonly buffer Prims { Prim p[]; };

layout(push_constant, scalar) uniform Push {
    vec2 screen;
    Prims prims;
} pc;
#define PRIM(i) pc.prims.p[i]
#define VERTEX_ID gl_VertexIndex
#define INSTANCE_ID gl_InstanceIndex
#endif

const uint KIND_BOX = 0u;
const uint KIND_SHADOW = 1u;
const uint KIND_COVERAGE = 2u;
const uint KIND_LINE = 3u;
const uint KIND_QUAD = 4u;
