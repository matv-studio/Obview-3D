#version 460
// Sélection au pixel : indice du triangle (dans l'ordre du tracé) et marqueur de présence.
layout(location = 0) in flat uint v_triangle;
layout(location = 0) out uvec2 o_id;

void main() {
    o_id = uvec2(v_triangle, 1u);
}
