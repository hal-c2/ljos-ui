#version 440
// What a ShaderEffect without a fragmentShader runs: its `source` times
// its opacity.
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
};
layout(binding = 1) uniform sampler2D source;
void main() {
    fragColor = texture(source, qt_TexCoord0) * qt_Opacity;
}
