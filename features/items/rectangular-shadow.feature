# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-effects-rectangularshadow.html
# Verified on Qt 6.11.2 (qml6). Qt's software scene graph, which the Qt
# backend uses, draws no RectangularShadow (measured: every pixel is the
# background), so what Ljós paints is pinned on Ljós only, against what
# qml6 drew with OpenGL in the desktop:cua sandbox (a grabToImage of the
# windows below): the edge of the shadow is half the colour, and it fades
# out over `blur` pixels on each side of it. Where the blur is short the
# edge is steep, so a pixel there is allowed more.

Feature: RectangularShadow
  A RectangularShadow draws a rectangle with soft edges behind what it is
  placed under, as a box shadow: its own rectangle moved by `offset`,
  grown by `spread` on every side, its corners rounded by `radius`, and
  blurred over `blur` pixels in `color`.

  @qt-parity
  Scenario Outline: Defaults
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Effects
      Item {
        width: 200; height: 200
        RectangularShadow { objectName: "shadow"; width: 100; height: 100 }
      }
      """
    Then the property "<property>" of "shadow" is <value>

    Examples:
      | property       | value     |
      | blur           | 10        |
      | spread         | 0         |
      | radius         | 0         |
      | color          | "#000000" |
      | cached         | false     |
      | offset.x       | 0         |
      | offset.y       | 0         |
      | implicitWidth  | 0         |
      | implicitHeight | 0         |

  @ljos-only
  Rule: What it paints

    Scenario: The edges of the shadow fade over its blur
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Effects
        Rectangle {
          width: 200; height: 200; color: "#ffffff"
          RectangularShadow { x: 50; y: 50; width: 100; height: 100 }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 100, 100 of the window is "#000000"
      And the pixel at 30, 100 of the window is "#ffffff"
      And the pixel at 45, 100 of the window is within 32 of "#e3e3e3"
      And the pixel at 50, 100 of the window is within 32 of "#8d8d8d"
      And the pixel at 55, 100 of the window is within 32 of "#212121"

    Scenario: The shadow is moved, grown and rounded
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Effects
        Rectangle {
          width: 200; height: 200; color: "#ffffff"
          RectangularShadow {
            x: 50; y: 50; width: 100; height: 100
            blur: 4; spread: 5; radius: 30
            offset: Qt.vector2d(20, 10); color: "#0000ff"
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 70, 110 of the window is "#0000ff"
      And the pixel at 60, 110 of the window is "#ffffff"
      And the pixel at 65, 110 of the window is within 64 of "#9191ff"
      And the pixel at 174, 110 of the window is within 64 of "#9191ff"
      And the pixel at 70, 60 of the window is "#ffffff"
