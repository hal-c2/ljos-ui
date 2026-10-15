# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-icons.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#sourceSize-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An SVG drawing as the icon of a button
  A drawing has no pixels of its own: a button draws it at its icon size,
  `icon.width` by `icon.height`, stretched to exactly that whatever the
  drawing's own size and shape, and in the icon's colour.

  Background:
    Given the text file "sq16.svg":
      """xml
      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16">
        <rect width="16" height="16" fill="#ff0000"/>
      </svg>
      """
    And the text file "sq48.svg":
      """xml
      <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48">
        <rect width="48" height="48" fill="#ff0000"/>
      </svg>
      """
    And the text file "half.svg":
      """xml
      <svg xmlns="http://www.w3.org/2000/svg" width="64" height="32">
        <rect width="32" height="32" fill="#ff0000"/>
      </svg>
      """

  Rule: The drawing is made at the icon size

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 400; color: "white"
          Button { objectName: "small"; icon.source: "sq16.svg" }
          Button { objectName: "big"; y: 50; icon.source: "sq48.svg" }
          Button { objectName: "long"; y: 100; icon.source: "half.svg" }
          Button {
            objectName: "natural"; y: 150
            icon.source: "sq16.svg"; icon.width: 0; icon.height: 0
          }
          Button {
            objectName: "large"; y: 200
            icon.source: "half.svg"; icon.width: 40; icon.height: 40
          }
          Button { objectName: "narrow"; y: 260; icon.source: "sq48.svg"; icon.width: 12 }
          Button { objectName: "bare"; y: 310; icon.source: "sq16" }
        }
        """

    @api-QtQuick.Controls.AbstractButton.icon.source @api-QtQuick.Controls.AbstractButton.icon.width
    Scenario Outline: The drawn icon of <button> is <width> by <height>
      Then the property "implicitContentWidth" of "<button>" is <width>
      And the property "implicitContentHeight" of "<button>" is <height>

      Examples:
        | button  | width | height |
        | small   | 24    | 24     |
        | big     | 24    | 24     |
        | long    | 24    | 24     |
        | natural | 16    | 16     |
        | large   | 40    | 40     |
        | narrow  | 12    | 24     |
        | bare    | 24    | 24     |

    Scenario: A small drawing is made larger, to fill the icon size
      Then the pixel at 39, 9 of the window is "#26282a"
      And the pixel at 60, 30 of the window is "#26282a"
      And the pixel at 37, 20 of the window is "#e0e0e0"
      And the pixel at 62, 20 of the window is "#e0e0e0"

  Rule: The drawing is stretched to the icon's shape and drawn in its colour

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 300; color: "white"
          Button { objectName: "half"; icon.source: "half.svg"; icon.color: "#00ff00" }
          Button { objectName: "asIs"; y: 50; icon.source: "half.svg"; icon.color: "transparent" }
        }
        """

    @api-QtQuick.Controls.AbstractButton.icon.color
    Scenario Outline: The left half of the drawing fills the left half of the icon
      Then the pixel at <x>, <y> of the window is "<colour>"

      Examples:
        | x  | y  | colour  |
        | 40 | 10 | #00ff00 |
        | 47 | 29 | #00ff00 |
        | 53 | 10 | #e0e0e0 |
        | 59 | 29 | #e0e0e0 |
        | 40 | 60 | #ff0000 |
        | 56 | 60 | #e0e0e0 |

  Rule: An icon with less room than its size is made smaller to fit, keeping its shape

    Background:
      Given a 48 by 48 image file "red48.png" filled with "#ff0000"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 400; color: "white"
          Button {
            width: 48; height: 38; display: AbstractButton.IconOnly
            icon.source: "sq16.svg"; icon.width: 38; icon.height: 38; icon.color: "transparent"
          }
          Button {
            y: 50; width: 30; height: 80; display: AbstractButton.IconOnly
            icon.source: "red48.png"; icon.width: 38; icon.height: 38; icon.color: "transparent"
          }
          Button {
            y: 140; width: 48; height: 38; display: AbstractButton.IconOnly
            icon.source: "half.svg"; icon.width: 38; icon.height: 38; icon.color: "transparent"
          }
        }
        """

    Scenario Outline: The icon fills the room in the button and no more, at <x>, <y>
      Then the pixel at <x>, <y> of the window is "<colour>"

      Examples:
        | x  | y   | colour  |
        | 11 | 6   | #ff0000 |
        | 36 | 31  | #ff0000 |
        | 10 | 20  | #e0e0e0 |
        | 37 | 20  | #e0e0e0 |
        | 20 | 5   | #e0e0e0 |
        | 20 | 32  | #e0e0e0 |
        | 8  | 83  | #ff0000 |
        | 21 | 96  | #ff0000 |
        | 7  | 90  | #e0e0e0 |
        | 22 | 90  | #e0e0e0 |
        | 15 | 82  | #e0e0e0 |
        | 15 | 97  | #e0e0e0 |
        | 11 | 146 | #ff0000 |
        | 23 | 171 | #ff0000 |
        | 24 | 150 | #e0e0e0 |

  Rule: Every kind of button draws its icon at the icon size

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 300; color: "white"
          ToolButton { objectName: "tool"; icon.source: "sq16.svg" }
          TabBar {
            y: 100; width: 200
            TabButton { objectName: "tab"; text: "on"; icon.source: "sq48.svg" }
          }
        }
        """

    Scenario: A tool button draws its icon at the icon size
      Then the property "implicitContentWidth" of "tool" is 24
      And the property "implicitContentHeight" of "tool" is 24
      And the pixel at 20, 20 of the window is "#26282a"
      And the pixel at 6, 20 of the window is within 1 of "#f0f0f0"

    Scenario: A tab button draws its icon at the icon size
      Then the property "implicitContentHeight" of "tab" is 24
