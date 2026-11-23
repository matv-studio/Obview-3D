// Corps du nuanceur de surfaces, partagé par `scene.frag` et `scene_section.frag` (SECTION défini : coupe
// et bouchons).
#extension GL_GOOGLE_include_directive : require
#include "scene_common.glsl"

#ifdef OBV_GL
layout(binding = 1) uniform sampler2D env_map;
layout(binding = 2) uniform sampler2DArray textures;
#else
layout(set = 0, binding = 0) uniform sampler2D env_map;
layout(set = 0, binding = 1) uniform sampler2DArray textures;
#endif
#define LIGHT PARAMS
#include "light.glsl"

layout(location = 0) in vec3 v_normal;
layout(location = 1) in vec3 v_world;
layout(location = 2) in flat uint v_material;
layout(location = 3) in vec3 v_bary;
layout(location = 4) in vec2 v_uv;
layout(location = 5) in flat uint v_part;
layout(location = 0) out vec4 out_color;

vec3 srgb_to_linear(vec3 c) {
    return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(0.04045, c));
}

void main() {
    if (outside_band(gl_FragCoord.xy)) discard;
    // Matériau (D-37) : facteurs, puis textures de couleur (sRGB) et métal-rugosité (B, G ; linéaires).
    Material mat = MATERIAL(v_material);
    vec3 base = mat.base.rgb;
    float metal = mat.metallic;
    float rough = mat.roughness;
    if (mat.base_tex >= 0) {
        base *= srgb_to_linear(texture(textures, vec3(v_uv, float(mat.base_tex))).rgb);
    }
    if (mat.mr_tex >= 0) {
        vec4 mr = texture(textures, vec3(v_uv, float(mat.mr_tex)));
        rough *= mr.g;
        metal *= mr.b;
    }
    metal = clamp(metal, 0.0, 1.0);
    rough = clamp(rough, 0.0, 1.0);
    // Pièce sélectionnée : teinte d'accent.
    if (v_part == PARAMS.selected) {
        base = mix(base, srgb_to_linear(unpackUnorm4x8(PARAMS.accent).rgb), 0.55);
        metal *= 0.45;
    }
    vec3 n = normalize(v_normal);
    vec3 v = to_eye(v_world);
#ifdef SECTION
    if (cut_away(v_world)) discard;
    if (!gl_FrontFacing) {
        // Face arrière vue à travers la coupe : intérieur de la matière, rendu comme le bouchon posé sur le
        // plan (normale du plan, hachures à 45° à l'écran). Elle garde sa propre profondeur : toute face avant
        // située entre le plan et elle la cache, comme la matière le ferait.
        vec3 pn = PARAMS.plane.xyz;
        float hatch = step(0.5, fract((gl_FragCoord.x + gl_FragCoord.y) / 9.0));
        vec3 cap = shade(base, pn, v) * mix(0.62, 0.48, hatch);
        out_color = vec4(cap, 1.0);
        return;
    }
#endif
    if (!gl_FrontFacing) n = -n;
    vec3 color;
    if (pc.mode == MODE_ALBEDO) {
        // Albédo du flux spéculaire (Marmoset) : la part diffuse de la couleur.
        color = base * (1.0 - metal);
    } else if (pc.mode == MODE_REFLECTIVITY) {
        // Couleur spéculaire à incidence normale, telle quelle.
        color = mix(vec3(0.04), base, metal);
    } else if (pc.mode == MODE_GLOSS) {
        // Brillance affichée comme une carte de données : la valeur codée est 1 - rugosité.
        color = srgb_to_linear(vec3(1.0 - rough));
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
        color = shade_pbr(base, metal, rough, mat.emissive, n, v);
    }
    out_color = vec4(color, 1.0);
}
