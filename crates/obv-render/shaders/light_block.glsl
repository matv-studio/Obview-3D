// Bloc d'éclairage de l'image (D-33), même disposition que `light::LightBlock` (20 vec4) :
// light_rot = (cos, sin de la rotation du dôme, gain, dernier niveau de la pyramide) ; light_sh : harmoniques
// de l'éclairement / π ; lumières directionnelles (direction dans le repère du dôme, w : demi-angle ; couleur :
// éclairement) ; light_bg = (fond environnement, niveau de flou, nombre de lumières, -) ; light_tint : teinte.
#define LIGHT_BLOCK \
    vec4 light_rot; \
    vec4 light_sh[9]; \
    vec4 light_dir[4]; \
    vec4 light_color[4]; \
    vec4 light_bg; \
    vec4 light_tint;

