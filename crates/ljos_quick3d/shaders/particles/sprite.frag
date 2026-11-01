VARYING vec4 tint;
VARYING vec2 place;
VARYING vec2 look;
VARYING float row;

vec4 pictured_at(float frame)
{
    float across = clamp(frame / frames, 0.0, 1.0) + fract(place.x) / frames;
    return texture(spritePicture, vec2(across, place.y));
}

void MAIN()
{
    vec4 color = tint;
    // Both are read whether they are used or not: a shader may not sample in
    // a branch that differs from one fragment to another.
    ivec2 size = textureSize(spriteTable, 0);
    vec2 inTable = vec2(fract(look.x), row) * vec2(size);
    vec4 tabledColor = texelFetch(spriteTable, min(ivec2(inTable), size - ivec2(1)), 0);
    float frame = (frames - interpolate) * look.y;
    float between = fract(frame);
    vec4 first = pictured_at(frame - between);
    vec4 second = pictured_at(frame - between + 1.0);
    if (tabled) {
        color *= tabledColor;
    }
    if (pictured) {
        color *= mix(first, second, between * interpolate);
    }
    // Multiplying has nothing to lay over: what has no alpha is white.
    if (multiply) {
        color = vec4(mix(vec3(1.0), color.rgb, color.a), 1.0);
    } else {
        color.rgb *= color.a;
    }
    FRAGCOLOR = color;
}
