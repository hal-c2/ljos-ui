// Only light over the threshold glows.
void MAIN()
{
    vec4 c = texture(INPUT, INPUT_UV);
    float luminance = dot(c.rgb, vec3(0.2126, 0.7152, 0.0722));
    FRAGCOLOR = vec4(c.rgb * step(bloomThreshold, luminance), 1.0);
}
