// A row is where the sprite is, how it is turned, and in its custom data its
// size, how far through its life it is and how far through its pictures.
VARYING vec4 tint;
VARYING vec2 place;
VARYING vec2 look;
VARYING float row;

void MAIN()
{
    tint = INSTANCE_COLOR;
    place = UV0;
    look = INSTANCE_DATA.yz;
    row = fract(sin(float(INSTANCE_INDEX) * 12.9898) * 43758.5453);
    vec3 centre = INSTANCE_MODEL_MATRIX[3].xyz;
    float size = INSTANCE_DATA.x;
    if (billboard) {
        vec4 seen = VIEW_MATRIX * vec4(centre, 1.0);
        seen.xy += VERTEX.xy / 100.0 * size;
        POSITION = PROJECTION_MATRIX * seen;
    } else {
        // How a sprite is turned is its own: what the system's scale and
        // turn did to the row is taken out again, as a sprite is as big as
        // it is told wherever its system is.
        mat3 own = inverse(mat3(MODEL_MATRIX)) * mat3(INSTANCE_MODEL_MATRIX);
        POSITION = VIEWPROJECTION_MATRIX * vec4(centre + own * (VERTEX / 100.0 * size), 1.0);
    }
}
