#version 460
#extension GL_GOOGLE_include_directive : require
#include "scene_common.glsl"

// Sélection au pixel : indice du triangle (dans l'ordre du tracé) et marqueur : 1 face avant, 2 face arrière
// vue à travers la coupe (bouchon : le point est ramené sur le plan côté processeur). Partie retirée ignorée.
layout(location = 0) in flat uint v_triangle;
layout(location = 1) in vec3 v_world;
layout(location = 0) out uvec2 o_id;

void main() {
    if (cut_away(v_world)) discard;
    o_id = uvec2(v_triangle, (section_on() && !gl_FrontFacing) ? 2u : 1u);
}
