# Sources:
#   docs/ARCHITECTURE.md (section 7.1: R, the reference renderer on
#   vello_cpu, and the window it draws on the processor)
#   https://wayland.app/protocols/wayland#wl_shm
#   https://wayland.app/protocols/wayland#wl_surface:request:damage_buffer
#   https://doc.qt.io/qt-6/qml-qtquick-shadereffect.html

# How a window is drawn is Ljós's own: qml6's software scene graph draws
# with its own rasteriser and logs nothing about effects it leaves out.
@ljos-only @desktop
Feature: A window drawn on the processor
  A window can be drawn on the processor into memory it shares with the
  window system, with no graphics driver at all. It shows what the
  reference renderer draws. After a change only the part that changed is
  drawn again, and the window is the same as if all of it had been. What
  the reference renderer cannot draw, a ShaderEffect's own program, is
  left out, and the log says so once.

  Background:
    Given the window is drawn on the processor

  Scenario: The window is the reference renderer's picture
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 120; color: "#204060"
        Rectangle {
          x: 10; y: 10; width: 80; height: 50; radius: 12
          border.width: 3; border.color: "#ffcc00"
          gradient: Gradient {
            GradientStop { position: 0; color: "#ff0000" }
            GradientStop { position: 1; color: "#0000ff" }
          }
        }
        Text { x: 100; y: 20; text: "Ljós"; color: "white"; font.pixelSize: 24 }
        Rectangle { x: 120; y: 70; width: 60; height: 30; color: "#8000ff00"; rotation: 15 }
      }
      """
    Then the window's picture is within 2 of the reference renderer's

  Scenario: A change is drawn again where it was and where it is now
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 120; color: "#000080"
        Rectangle { objectName: "box"; x: 10; y: 10; width: 40; height: 40; color: "#ff0000" }
        Rectangle { x: 150; y: 70; width: 40; height: 40; color: "#00ff00" }
      }
      """
    When the property "x" of "box" is set to 100
    Then the pixel at 30, 30 of the window is "#000080"
    And the pixel at 120, 30 of the window is "#ff0000"
    And the window's picture is within 2 of the reference renderer's
    When the property "x" of "box" is set to 60
    And the property "color" of "box" is set to "#ffff00"
    Then the pixel at 120, 30 of the window is "#000080"
    And the pixel at 80, 30 of the window is "#ffff00"
    And the pixel at 170, 90 of the window is "#00ff00"
    And the window's picture is within 2 of the reference renderer's

  Scenario: A list scrolled in a clip shows each row where it now is
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 200; color: "#000080"
        ListView {
          objectName: "list"
          x: 20; y: 20; width: 160; height: 120
          clip: true
          model: 20
          delegate: Rectangle {
            width: 160; height: 40
            color: ["#ff0000", "#00ff00", "#0000ff", "#ffff00", "#ff00ff"][index % 5]
          }
        }
      }
      """
    When the property "contentY" of "list" is set to 7
    And the property "contentY" of "list" is set to 61
    Then the pixel at 100, 21 of the window is "#00ff00"
    And the pixel at 100, 139 of the window is "#ff00ff"
    And the pixel at 100, 141 of the window is "#000080"
    When the property "contentY" of "list" is set to 23
    Then the pixel at 100, 21 of the window is "#ff0000"
    And the window's picture is within 2 of the reference renderer's

  Scenario: A ShaderEffect's program is left out, and said once
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
        ShaderEffect { x: 60; width: 40; height: 40; fragmentShader: "red.frag.qsb" }
      }
      """
    When 100 milliseconds pass
    Then the pixel at 20, 20 of the window is "#000080"
    And the log has 1 warnings matching "drawn on the processor: a ShaderEffect's program is left out"

  # An X server keeps nothing of a window that is not mapped, and the window
  # is mapped once there is a picture in it: that first picture is lost, and
  # the redraw the server asks for may arrive as one the scene asked for too.
  # This scene's layout asks for that pass in time. A window that stays blank
  # is not mapped by XWayland at all, so the step that starts it says so.
  Scenario: A picture drawn before the window was mapped is drawn again after
    Given the shell chooses how the window is drawn
    And the application is a client of an X server
    And the text file "Hero.qml":
      """qml
      import QtQuick
      import Ljos.Design
      Rectangle {
        id: hero
        property bool compact; property real gutter
        width: parent.width
        height: copy.height + 2 * (compact ? Spacing.xxl : Spacing.xxxl * 2)
        color: "#204060"
        Column {
          id: copy; anchors.centerIn: parent
          width: Math.min(parent.width - 2 * hero.gutter, 720)
          spacing: Spacing.lg
          Text {
            width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap
            text: "Words that wrap once their column has got its width"; color: Colors.text
            font.pixelSize: hero.compact ? Typography.headlineSize : Typography.displaySize
            font.weight: Typography.displayWeight
          }
          Text {
            width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap
            text: "More words below them, as many again and then some, so that this text also takes a second line here."
            color: Colors.textMuted; font.pixelSize: Typography.subtitleSize
          }
        }
      }
      """
    And the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Rectangle {
        id: root
        width: 1280; height: 800; color: Colors.canvas
        property int columns: 3
        property bool compact: false
        readonly property real gutter: compact ? Spacing.md : Spacing.xxl
        Flickable {
          id: page
          anchors.fill: parent; contentHeight: sections.height; clip: true
          Column {
            id: sections
            width: page.width
            Hero { compact: root.compact; gutter: root.gutter }
          }
        }
      }
      """
    Then the pixel at 5, 5 of the window is "#204060"
