# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-busyindicator.html
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html#customizing-busyindicator
# Verified on Qt 6.11.2 (qml6, offscreen): the properties read from the
# item, the dots from pictures of the window taken while it ran.

@qt-parity
Feature: The Basic style's busy ring
  `QtQuick.Controls.Basic.impl` has the ring the Basic busy indicator is
  drawn with, so a custom indicator can draw it in its own colours. It is
  ten dots in a circle, outlined in `pen`. While it runs the dots fill
  with `fill` one after another clockwise from the top, then empty in the
  same order. It has no size of its own.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic.impl
      Rectangle {
        width: 100; height: 100; color: "white"
        BusyIndicatorImpl {
          objectName: "b"; width: 68; height: 68
          pen: "#1c44b1"; fill: "#ff0000"; running: true
        }
      }
      """

  Scenario Outline: The ring's <property> is <value>
    Then the property "<property>" of "b" is <value>

    Examples:
      | property       | value     |
      | implicitWidth  | 0         |
      | implicitHeight | 0         |
      | pen            | "#1c44b1" |
      | fill           | "#ff0000" |
      | running        | true      |

  Scenario: The dots fill clockwise from the top
    When 250 milliseconds pass
    Then the pixel at 34, 4 of the window is "#ff0000"
    And the pixel at 51, 10 of the window is "#ff0000"
    And the pixel at 34, 63 of the window is "#ffffff"
    And the pixel at 5, 43 of the window is "#ffffff"
