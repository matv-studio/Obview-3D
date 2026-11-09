#version 460

layout(location = 0) in vec3 in_normal;
layout(location = 0) out vec4 out_color;

void main() {
    vec3 n = normalize(in_normal);
    vec3 l = normalize(vec3(0.4, 0.8, 0.5));
    float diffuse = max(dot(n, l), 0.0);
    float hemi = 0.5 + 0.5 * n.y;
    vec3 base = vec3(0.75, 0.78, 0.82);
    out_color = vec4(base * (0.12 + 0.25 * hemi + 0.68 * diffuse), 1.0);
}
