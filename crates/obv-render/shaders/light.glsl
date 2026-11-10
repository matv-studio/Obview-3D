// Ombrage à base physique sous un environnement (D-33). Avant l'inclusion : `LIGHT` (bloc des paramètres qui
// porte LIGHT_BLOCK) et l'échantillonneur `env_map` (pyramide équirectangulaire préfiltrée, voir light.rs).

const float PI = 3.14159265359;

// Repère du monde vers celui du dôme (rotation de −θ autour de Y), et retour.
vec3 to_env(vec3 d) {
    float c = LIGHT.light_rot.x, s = LIGHT.light_rot.y;
    return vec3(c * d.x - s * d.z, d.y, s * d.x + c * d.z);
}
vec3 from_env(vec3 d) {
    float c = LIGHT.light_rot.x, s = LIGHT.light_rot.y;
    return vec3(c * d.x + s * d.z, d.y, -s * d.x + c * d.z);
}

// u = atan2(x, −z) / 2π + ½, v = acos(y) / π (v = 0 en haut), comme `light::uv_of`.
vec2 equirect(vec3 d) {
    return vec2(atan(d.x, -d.z) / (2.0 * PI) + 0.5, acos(clamp(d.y, -1.0, 1.0)) / PI);
}

// Radiance de l'environnement vue dans la direction `d` du monde, au niveau de flou `lod`.
vec3 env_radiance(vec3 d, float lod) {
    return textureLod(env_map, equirect(to_env(d)), lod).rgb;
}

// Éclairement / π dans la direction `n` du dôme (harmoniques d'ordre 2, même base que `light::sh_basis`).
vec3 sh_irradiance(vec3 n) {
    float x = n.x, y = n.y, z = n.z;
    vec3 e = LIGHT.light_sh[0].rgb * 0.282095
        + LIGHT.light_sh[1].rgb * (0.488603 * y)
        + LIGHT.light_sh[2].rgb * (0.488603 * z)
        + LIGHT.light_sh[3].rgb * (0.488603 * x)
        + LIGHT.light_sh[4].rgb * (1.092548 * x * y)
        + LIGHT.light_sh[5].rgb * (1.092548 * y * z)
        + LIGHT.light_sh[6].rgb * (0.315392 * (3.0 * z * z - 1.0))
        + LIGHT.light_sh[7].rgb * (1.092548 * x * z)
        + LIGHT.light_sh[8].rgb * (0.546274 * (x * x - y * y));
    return max(e, vec3(0.0));
}

// Terme spéculaire intégré de l'environnement (approximation analytique de Karis).
vec2 env_brdf(float nv, float rough) {
    const vec4 c0 = vec4(-1.0, -0.0275, -0.572, 0.022);
    const vec4 c1 = vec4(1.0, 0.0425, 1.04, -0.04);
    vec4 r = rough * c0 + c1;
    float a004 = min(r.x * r.x, exp2(-9.28 * nv)) * r.x + r.y;
    return vec2(-1.04, 1.04) * a004 + r.zw;
}

// Correspondance de tons « PBR Neutral » de Khronos (teintes des produits gardées jusqu'aux hautes lumières).
vec3 tone_map(vec3 c) {
    const float start = 0.8 - 0.04;
    const float desat = 0.15;
    float x = min(c.r, min(c.g, c.b));
    float offset = x < 0.08 ? x - 6.25 * x * x : 0.04;
    c -= offset;
    float peak = max(c.r, max(c.g, c.b));
    if (peak < start) return c;
    const float d = 1.0 - start;
    float new_peak = 1.0 - d * d / (peak + d - start);
    c *= new_peak / peak;
    float g = 1.0 - 1.0 / (desat * (peak - new_peak) + 1.0);
    return mix(c, vec3(new_peak), g);
}

// Matière diélectrique satinée (plastique, peinture) : couleur de base, rugosité 0,42, F0 = 0,04.
vec3 shade(vec3 base, vec3 n, vec3 v) {
    const float rough = 0.42;
    const vec3 f0 = vec3(0.04);
    float nv = max(dot(n, v), 1e-4);
    vec2 ab = env_brdf(nv, rough);
    vec3 fs = f0 * ab.x + ab.y;
    vec3 color = base * sh_irradiance(to_env(n)) * (1.0 - fs);
    color += env_radiance(reflect(-v, n), rough * LIGHT.light_rot.w) * fs;
    int count = int(LIGHT.light_bg.z + 0.5);
    for (int i = 0; i < 4; i++) {
        if (i >= count) break;
        vec3 l = from_env(LIGHT.light_dir[i].xyz);
        float nl = dot(n, l);
        if (nl <= 0.0) continue;
        // Source étendue : rugosité élargie par son demi-angle (reflet du soleil pas plus fin que le disque).
        float a = clamp(rough * rough + LIGHT.light_dir[i].w, 0.002, 1.0);
        float a2 = a * a;
        vec3 h = normalize(l + v);
        float nh = max(dot(n, h), 0.0);
        float vh = max(dot(v, h), 0.0);
        float dd = nh * nh * (a2 - 1.0) + 1.0;
        float D = a2 / (PI * dd * dd);
        float vis = 0.5 / (nl * sqrt(nv * nv * (1.0 - a2) + a2) + nv * sqrt(nl * nl * (1.0 - a2) + a2));
        vec3 F = f0 + (1.0 - f0) * pow(1.0 - vh, 5.0);
        vec3 E = LIGHT.light_color[i].rgb * nl;
        color += (base * (1.0 - F) / PI + D * vis * F) * E;
    }
    return tone_map(color * LIGHT.light_tint.rgb * LIGHT.light_rot.z);
}

// Fond : environnement dans la direction `d`, flou du réglage, même exposition que la scène.
vec3 env_background(vec3 d) {
    return tone_map(env_radiance(d, LIGHT.light_bg.y) * LIGHT.light_tint.rgb * LIGHT.light_rot.z);
}
