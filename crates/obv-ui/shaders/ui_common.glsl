// Primitives de l'interface (voir paint.rs : Prim).
#extension GL_EXT_buffer_reference : require
#extension GL_EXT_scalar_block_layout : require

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

layout(buffer_reference, scalar, buffer_reference_align = 16) readonly buffer Prims { Prim p[]; };

layout(push_constant, scalar) uniform Push {
    vec2 screen;
    Prims prims;
} pc;

const uint KIND_BOX = 0u;
const uint KIND_SHADOW = 1u;
const uint KIND_COVERAGE = 2u;
const uint KIND_LINE = 3u;
const uint KIND_QUAD = 4u;
