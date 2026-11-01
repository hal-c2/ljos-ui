// A row is one piece of a line: its first column goes from the piece's middle
// to its end, its second is the way it is wide, and its custom data has the
// widths at its two ends and how far along the line those ends are.
VARYING vec4 tint;
VARYING vec2 place;
VARYING vec2 look;
VARYING float row;

void MAIN()
{
    tint = INSTANCE_COLOR;
    float along = VERTEX.x / 100.0 + 0.5;
    float side = VERTEX.y / 100.0;
    place = vec2(mix(INSTANCE_DATA.z, INSTANCE_DATA.w, along), UV0.y);
    look = vec2(0.0);
    row = 0.0;
    vec3 middle = INSTANCE_MODEL_MATRIX[3].xyz;
    vec3 way = INSTANCE_MODEL_MATRIX[0].xyz * VERTEX.x;
    float width = mix(INSTANCE_DATA.x, INSTANCE_DATA.y, along);
    if (billboard) {
        vec4 seen = VIEW_MATRIX * vec4(middle + way, 1.0);
        vec2 aside = (mat3(VIEW_MATRIX) * INSTANCE_MODEL_MATRIX[1].xyz).xy;
        float long = length(aside);
        if (long > 0.0) {
            seen.xy += aside / long * side * width;
        }
        POSITION = PROJECTION_MATRIX * seen;
    } else {
        POSITION = VIEWPROJECTION_MATRIX * vec4(middle + way + INSTANCE_MODEL_MATRIX[1].xyz * side * width, 1.0);
    }
}
