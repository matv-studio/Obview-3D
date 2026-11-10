#version 460
#extension GL_GOOGLE_include_directive : require
#include "ui_common.glsl"

layout(location = 0) out flat uint v_index;
layout(location = 1) out vec2 v_pos;

const vec2 CORNERS[6] = vec2[](vec2(0, 0), vec2(1, 0), vec2(0, 1), vec2(1, 0), vec2(1, 1), vec2(0, 1));

void main() {
    Prim p = PRIM(INSTANCE_ID);
    uint kind = uint(p.params.z);
    vec2 lo, hi;
    if (kind == KIND_LINE) {
        float h = p.params.x * 0.5 + 1.0;
        lo = min(p.rect.xy, p.rect.zw) - h;
        hi = max(p.rect.xy, p.rect.zw) + h;
    } else if (kind == KIND_QUAD) {
        lo = min(min(p.rect.xy, p.rect.zw), min(p.radii.xy, p.radii.zw)) - 1.0;
        hi = max(max(p.rect.xy, p.rect.zw), max(p.radii.xy, p.radii.zw)) + 1.0;
    } else if (kind == KIND_SHADOW) {
        float e = p.params.y * 3.0;
        lo = p.rect.xy - e;
        hi = p.rect.zw + e;
    } else if (kind == KIND_COVERAGE) {
        lo = p.rect.xy;
        hi = p.rect.zw;
    } else {
        lo = p.rect.xy - 1.0;
        hi = p.rect.zw + 1.0;
    }
    bool rotated = kind == KIND_COVERAGE && p.border.w > 0.5;
    if (!rotated) {
        // Découpe : la géométrie elle-même est réduite au rectangle de découpe.
        lo = max(lo, p.clip.xy);
        hi = max(min(hi, p.clip.zw), lo);
    }
    vec2 pos = mix(lo, hi, CORNERS[VERTEX_ID]);
    v_index = uint(INSTANCE_ID);
    // Le fragment lit l'atlas d'après la position d'origine ; seule la position à l'écran est transformée.
    v_pos = pos;
    if (rotated) {
        vec4 m = p.radii;
        vec2 d = pos - p.border.yz;
        pos = p.border.yz + vec2(m.x * d.x + m.y * d.y, m.z * d.x + m.w * d.y);
    }
    gl_Position = vec4(pos / pc.screen * 2.0 - 1.0, 0.0, 1.0);
#ifdef OBV_GL
    // Repère de découpage d'OpenGL : y vers le haut.
    gl_Position.y = -gl_Position.y;
#endif
}
