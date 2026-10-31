void MAIN()
{
    vec4 c = texture(INPUT, INPUT_UV);
    vec3 glowing = texture(glowTexture, INPUT_UV).rgb * tonemappingLerp / (1.0 + blurFalloff);
    // The white point is 0 at one end of its range: dividing by it must stay finite.
    vec3 light = (c.rgb + glowing) * pow(2.0, exposure) / max(channelThreshold, 0.001);
    FRAGCOLOR = vec4(pow(max(light, vec3(0.0)), vec3(1.0 / gamma)), c.a);
}
