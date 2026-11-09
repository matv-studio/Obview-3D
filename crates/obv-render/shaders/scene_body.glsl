// Corps du nuanceur de surfaces, partagé par `scene.frag` et `scene_section.frag` (SECTION défini : coupe
// et bouchons).
#extension GL_GOOGLE_include_directive : require
#include "scene_common.glsl"

layout(location = 0) in vec3 v_normal;
layout(location = 1) in vec3 v_world;
layout(location = 2) in flat uint v_color;
layout(location = 3) in vec3 v_bary;
layout(location = 0) out vec4 out_color;

vec3 srgb_to_linear(vec3 c) {
    return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(0.04045, c));
}

// Éclairage de présentation de pièces : ciel/sol, lumière principale, lumière d'appoint, contre-jour, reflet.
vec3 lit(vec3 base, vec3 n, vec3 v) {
    // Repère de la caméra : les lumières suivent la vue, comme dans les visionneuses CAO.
    vec3 fwd = -v;
    vec3 right = normalize(cross(fwd, vec3(0.0, 1.0, 0.0) + vec3(1e-4, 0.0, 0.0)));
    vec3 up = cross(right, fwd);
    vec3 key = normalize(-fwd * 0.6 + up * 0.65 - right * 0.45);
    vec3 fill = normalize(-fwd * 0.8 - up * 0.2 + right * 0.6);
    vec3 rim = normalize(fwd * 0.7 + up * 0.5);
    float hemi = 0.5 + 0.5 * dot(n, vec3(0.0, 1.0, 0.0));
    vec3 ambient = mix(vec3(0.20, 0.19, 0.18), vec3(0.42, 0.44, 0.47), hemi);
    float d_key = max(dot(n, key), 0.0);
    float d_fill = max(dot(n, fill), 0.0);
    float d_rim = pow(max(dot(n, rim), 0.0), 2.0);
    vec3 h = normalize(key + v);
    float shininess = 48.0;
    float spec = pow(max(dot(n, h), 0.0), shininess) * (shininess + 8.0) / 25.13;
    float fresnel = 0.04 + 0.96 * pow(1.0 - max(dot(n, v), 0.0), 5.0);
    return base * (ambient * 0.55 + d_key * 0.85 + d_fill * 0.28)
         + vec3(1.0) * (spec * 0.12 * (0.3 + 0.7 * fresnel))
         + base * d_rim * 0.18
         + vec3(fresnel * 0.06) * hemi;
}

void main() {
    if (outside_band(gl_FragCoord.xy)) discard;
    vec3 base = srgb_to_linear(unpackUnorm4x8(v_color).rgb);
    vec3 n = normalize(v_normal);
    vec3 v = to_eye(v_world);
#ifdef SECTION
    if (cut_away(v_world)) discard;
    if (!gl_FrontFacing) {
        // Face arrière vue à travers la coupe : intérieur de la matière, rendu comme le bouchon posé sur le
        // plan (normale du plan, hachures à 45° à l'écran). Elle garde sa propre profondeur : toute face avant
        // située entre le plan et elle la cache, comme la matière le ferait.
        vec3 pn = pc.params.plane.xyz;
        float hatch = step(0.5, fract((gl_FragCoord.x + gl_FragCoord.y) / 9.0));
        vec3 cap = lit(base, pn, v) * mix(0.62, 0.48, hatch);
        out_color = vec4(cap, 1.0);
        return;
    }
#endif
    if (!gl_FrontFacing) n = -n;
    vec3 color;
    if (pc.mode == MODE_COLOR) {
        color = base;
    } else if (pc.mode == MODE_NORMALS) {
        // Normale du monde affichée telle quelle : la cible sRGB réencode, d'où la conversion.
        color = srgb_to_linear(n * 0.5 + 0.5);
    } else if (pc.mode == MODE_TOPOLOGY) {
        // Surfaces mates, triangles tracés à largeur constante à l'écran.
        float shade = 0.35 + 0.65 * max(dot(n, v), 0.0);
        color = vec3(0.30) * shade;
        vec3 w = fwidth(v_bary);
        vec3 a = smoothstep(vec3(0.0), w * 1.1, v_bary);
        float edge = 1.0 - min(min(a.x, a.y), a.z);
        color = mix(color, vec3(0.80, 0.83, 0.88), edge * 0.9);
    } else if (pc.mode == MODE_EDGES) {
        // Dessin technique : surfaces presque blanches, ombrées à peine ; les arêtes sont tracées à part.
        float shade = 0.82 + 0.18 * max(dot(n, v), 0.0);
        color = vec3(0.92) * shade;
    } else {
        color = lit(base, n, v);
    }
    out_color = vec4(color, 1.0);
}
