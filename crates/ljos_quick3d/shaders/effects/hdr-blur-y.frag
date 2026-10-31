// Half of a wide Gaussian: the other pass does the other direction.
void MAIN()
{
    vec2 texel = 1.0 / vec2(textureSize(INPUT, 0));
    vec3 sum = vec3(0.0);
    float total = 0.0;
    for (int i = -8; i <= 8; i++) {
        float weight = exp(-float(i * i) / 18.0);
        sum += texture(INPUT, INPUT_UV + vec2(0.0, float(i)) * texel).rgb * weight;
        total += weight;
    }
    FRAGCOLOR = vec4(sum / total, 1.0);
}
