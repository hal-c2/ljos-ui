# Sources:
#   docs/ARCHITECTURE.md (section 7.1: the window drawn on the processor,
#   and when the shell chooses it)
#   docs/BENCHMARKS.md (SHM1)
#   https://wayland.app/protocols/linux-dmabuf-v1
#   https://doc.qt.io/qt-6/qml-qtquick-shadereffect.html

# What draws a window is Ljós's own choice: qml6 draws with its OpenGL
# scene graph on whatever EGL offers and says nothing of it.
@ljos-only @desktop
Feature: The processor draws where EGL would draw in software
  Where EGL would offer only a software rasteriser (no GPU the process may
  open, or a compositor that takes no buffers from one, as the test
  sandbox's), a window drawn by the reference renderer into shared memory
  takes less memory and shows its frames as soon. The shell chooses it
  then, and says so in the log. As soon as a window shows what only GL
  draws, such as a ShaderEffect's own program, every window goes to GL,
  so nothing is left out.

  Background:
    Given the shell chooses how the window is drawn

  Scenario: The window is drawn on the processor without being asked
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 120; color: "#204060"
        Rectangle { x: 10; y: 10; width: 80; height: 50; radius: 12; color: "#ffcc00" }
        Text { x: 100; y: 20; text: "Ljós"; color: "white"; font.pixelSize: 24 }
      }
      """
    Then the log has an info matching "drawn on the processor: EGL would draw in software"
    And the window's picture is within 2 of the reference renderer's
    And the log has no info matching "drawn with GL from now on"

  Scenario: A ShaderEffect takes the window to GL, which draws its program
    Given the text file "red.frag":
      """
      #version 440
      layout(location = 0) in vec2 qt_TexCoord0;
      layout(location = 0) out vec4 fragColor;
      layout(std140, binding = 0) uniform buf { mat4 qt_Matrix; float qt_Opacity; };
      void main() { fragColor = vec4(1.0, 0.0, 0.0, 1.0) * qt_Opacity; }
      """
    And the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 120; height: 60; color: "#000080"
        ShaderEffect { x: 0; width: 40; height: 40; fragmentShader: "red.frag.qsb" }
        Rectangle { x: 60; y: 10; width: 40; height: 40; radius: 10; color: "#00ff00" }
      }
      """
    When 300 milliseconds pass
    Then the pixel at 20, 20 of the window is "#ff0000"
    And the pixel at 80, 30 of the window is "#00ff00"
    And the pixel at 110, 55 of the window is "#000080"
    And the log has an info matching "drawn with GL from now on"
    And the log has no warning matching "a ShaderEffect's program is left out"
