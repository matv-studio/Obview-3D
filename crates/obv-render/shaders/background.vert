#version 460
#ifdef OBV_GL
#define VERTEX_ID gl_VertexID
#else
#define VERTEX_ID gl_VertexIndex
#endif
layout(location = 0) out vec2 v_uv;
void main() {
    // Triangle couvrant toute la fenêtre d'affichage.
    vec2 p = vec2((VERTEX_ID << 1) & 2, VERTEX_ID & 2);
    v_uv = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
