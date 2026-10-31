void MAIN()
{
    vec4 c = texture(INPUT, INPUT_UV);
    vec3 scaled = c.rgb * vec3(redStrength, greenStrength, blueStrength);
    float grey = dot(scaled, vec3(0.2126, 0.7152, 0.0722));
    FRAGCOLOR = vec4(mix(vec3(grey), scaled, 1.0 + saturation), c.a);
}
