#version 440
// What a ShaderEffect without a vertexShader runs: the item's rectangle,
// with qt_TexCoord0 from (0, 0) at its top left to (1, 1) at its bottom
// right, as the ShaderEffect documentation gives it.
layout(location = 0) in vec4 qt_Vertex;
layout(location = 1) in vec2 qt_MultiTexCoord0;
layout(location = 0) out vec2 qt_TexCoord0;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
};
void main() {
    qt_TexCoord0 = qt_MultiTexCoord0;
    gl_Position = qt_Matrix * qt_Vertex;
}
