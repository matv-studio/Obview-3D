#version 460
#extension GL_GOOGLE_include_directive : require
#include "ui_common.glsl"

layout(location = 0) in flat uint v_index;
layout(location = 1) in vec2 v_pos;
layout(location = 0) out vec4 out_color;
layout(set = 0, binding = 0) uniform sampler2D atlas;

// Distance signée à une boîte arrondie (rayons : haut-gauche, haut-droite, bas-droite, bas-gauche ; y vers le bas).
float sd_round_box(vec2 p, vec2 b, vec4 r) {
    float rad = (p.x > 0.0) ? ((p.y > 0.0) ? r.z : r.y) : ((p.y > 0.0) ? r.w : r.x);
    rad = min(rad, min(b.x, b.y));
    vec2 q = abs(p) - b + rad;
    return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - rad;
}

// Ombre d'une boîte arrondie floutée par une gaussienne (méthode d'Evan Wallace).
float gaussian(float x, float sigma) {
    return exp(-(x * x) / (2.0 * sigma * sigma)) / (2.5066282746 * sigma);
}
vec2 erf2(vec2 x) {
    vec2 s = sign(x), a = abs(x);
    x = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
    x *= x;
    return s - s / (x * x);
}
float shadow_x(float x, float y, float sigma, float corner, vec2 half_size) {
    float delta = min(half_size.y - corner - abs(y), 0.0);
    float curved = half_size.x - corner + sqrt(max(0.0, corner * corner - delta * delta));
    vec2 integral = 0.5 + 0.5 * erf2((x + vec2(-curved, curved)) * (0.70710678 / sigma));
    return integral.y - integral.x;
}
float rounded_box_shadow(vec2 lower, vec2 upper, vec2 point, float sigma, float corner) {
    vec2 center = (lower + upper) * 0.5;
    vec2 half_size = (upper - lower) * 0.5;
    point -= center;
    float low = point.y - half_size.y;
    float high = point.y + half_size.y;
    float start = clamp(-3.0 * sigma, low, high);
    float end = clamp(3.0 * sigma, low, high);
    float step = (end - start) / 6.0;
    float y = start + step * 0.5;
    float value = 0.0;
    for (int i = 0; i < 6; i++) {
        value += shadow_x(point.x, point.y - y, sigma, corner, half_size) * gaussian(y, sigma) * step;
        y += step;
    }
    return value;
}

void main() {
    Prim p = pc.prims.p[v_index];
    uint kind = uint(p.params.z);
    vec2 pos = v_pos;
    if (kind == KIND_COVERAGE) {
        vec2 t = (pos - p.rect.xy) / max(p.rect.zw - p.rect.xy, vec2(1e-5));
        float a = textureLod(atlas, mix(p.uv.xy, p.uv.zw, t), 0.0).r;
        out_color = p.color0 * pow(a, p.params.w);
        return;
    }
    if (kind == KIND_SHADOW) {
        float a = rounded_box_shadow(p.rect.xy, p.rect.zw, pos, p.params.y, p.radii.x);
        out_color = p.color0 * a;
        return;
    }
    if (kind == KIND_QUAD) {
        vec2 q[4] = vec2[](p.rect.xy, p.rect.zw, p.radii.xy, p.radii.zw);
        // Orientation des sommets, pour que la distance soit positive à l'intérieur.
        float area = 0.0;
        for (int i = 0; i < 4; i++) {
            vec2 a = q[i], b = q[(i + 1) % 4];
            area += a.x * b.y - b.x * a.y;
        }
        float sgn = area >= 0.0 ? 1.0 : -1.0;
        float dmin = 1e9;
        for (int i = 0; i < 4; i++) {
            vec2 a = q[i], b = q[(i + 1) % 4];
            vec2 e = b - a;
            float len = max(length(e), 1e-5);
            float dist = sgn * ((pos.x - a.x) * e.y - (pos.y - a.y) * e.x) / len;
            dmin = min(dmin, -dist);
        }
        // dmin > 0 à l'intérieur (en pixels).
        float outer = clamp(dmin + 0.5, 0.0, 1.0);
        float bw = p.params.x;
        if (bw > 0.0) {
            float inner = clamp(dmin - bw + 0.5, 0.0, 1.0);
            out_color = p.color0 * inner + p.border * (outer - inner);
        } else {
            out_color = p.color0 * outer;
        }
        return;
    }
    if (kind == KIND_LINE) {
        vec2 a = p.rect.xy, b = p.rect.zw;
        vec2 pa = pos - a, ba = b - a;
        float h = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-6), 0.0, 1.0);
        float d = length(pa - ba * h) - p.params.x * 0.5;
        out_color = p.color0 * clamp(0.5 - d, 0.0, 1.0);
        return;
    }
    vec2 c = (p.rect.xy + p.rect.zw) * 0.5;
    vec2 b = (p.rect.zw - p.rect.xy) * 0.5;
    float d = sd_round_box(pos - c, b, p.radii);
    float outer = clamp(0.5 - d, 0.0, 1.0);
    float t = clamp((pos.y - p.rect.y) / max(p.rect.w - p.rect.y, 1.0), 0.0, 1.0);
    vec4 fill = mix(p.color0, p.color1, t);
    float bw = p.params.x;
    if (bw > 0.0) {
        float inner = clamp(0.5 - (d + bw), 0.0, 1.0);
        out_color = fill * inner + p.border * (outer - inner);
    } else {
        out_color = fill * outer;
    }
}
