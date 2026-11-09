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
