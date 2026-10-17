# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html
#   https://doc.qt.io/qt-6/qtquickcontrols-icons.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-action.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The icon of a button
  A button shows a picture beside its text: `icon.source` names it,
  `icon.width` and `icon.height` say how large it may be, `icon.color`
  what colour it is drawn in, and `display` whether the picture, the
  text or both are shown.

  Rule: The icon is as large as its picture, and no larger than the icon size

    Background:
      Given a 16 by 16 image file "red.png" filled with "#ff0000"
      And a 48 by 48 image file "big.png" filled with "#ff0000"
      And a 64 by 32 image file "long.png" filled with "#ff0000"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 300; color: "white"
          Button { objectName: "small"; icon.source: "red.png" }
          Button { objectName: "big"; y: 50; icon.source: "big.png" }
          Button { objectName: "long"; y: 100; icon.source: "long.png" }
          Button {
            objectName: "natural"; y: 150
            icon.source: "big.png"; icon.width: 0; icon.height: 0
          }
          Button { objectName: "narrow"; y: 230; icon.source: "big.png"; icon.width: 12 }
          Button { objectName: "none"; x: 150 }
        }
        """

    @api-QtQuick.Controls.AbstractButton.icon.width @api-QtQuick.Controls.AbstractButton.icon.height
    Scenario Outline: The icon of <button> is <width> by <height>
      Then the property "implicitContentWidth" of "<button>" is <width>
      And the property "implicitContentHeight" of "<button>" is <height>

      Examples:
        | button  | width | height |
        | none    | 0     | 0      |
        | small   | 16    | 16     |
        | big     | 24    | 24     |
        | long    | 24    | 12     |
        | natural | 48    | 48     |
        | narrow  | 12    | 12     |

    Scenario: A button is no wider for an icon that fits in it
      Then the property "implicitWidth" of "small" is 100
      And the property "implicitHeight" of "small" is 40

    Scenario: A button grows for an icon that does not fit in it
      Then the property "implicitHeight" of "natural" is 60

    @api-QtQuick.Controls.AbstractButton.icon.source
    Scenario Outline: The icon is in the middle of a button without text
      Then the pixel at <x>, <y> of the window is "<colour>"

      Examples:
        | x  | y  | colour  |
        | 50 | 20 | #26282a |
        | 42 | 12 | #26282a |
        | 57 | 27 | #26282a |
        | 41 | 20 | #e0e0e0 |
        | 58 | 20 | #e0e0e0 |
        | 50 | 11 | #e0e0e0 |
        | 50 | 28 | #e0e0e0 |

    Scenario Outline: A picture larger than the icon size is made smaller
      Then the pixel at <x>, <y> of the window is "<colour>"

      Examples:
        | x  | y   | colour  |
        | 38 | 58  | #26282a |
        | 61 | 81  | #26282a |
        | 37 | 70  | #e0e0e0 |
        | 62 | 70  | #e0e0e0 |
        | 38 | 114 | #26282a |
        | 61 | 125 | #26282a |
        | 50 | 113 | #e0e0e0 |
        | 50 | 126 | #e0e0e0 |

    Scenario: An icon given later is shown
      When the property "icon.source" of "none" is set to "red.png"
      Then the property "implicitContentWidth" of "none" is 16
      And the pixel at 200, 20 of the window is "#26282a"

    Scenario: An icon taken away is gone
      When the property "icon.source" of "small" is set to ""
      Then the property "implicitContentWidth" of "small" is 0
      And the pixel at 50, 20 of the window is "#e0e0e0"

  Rule: The icon is drawn in one colour

    Background:
      Given a 16 by 16 image file "red.png" filled with "#ff0000"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 300; color: "white"
          Button { objectName: "plain"; icon.source: "red.png" }
          Button { objectName: "own"; y: 50; icon.source: "red.png"; icon.color: "#00ff00" }
          Button {
            objectName: "asIs"; y: 100
            icon.source: "red.png"; icon.color: "transparent"
          }
          Button { objectName: "bright"; y: 150; icon.source: "red.png"; highlighted: true }
          Button {
            objectName: "checked"; y: 200
            icon.source: "red.png"; checkable: true; checked: true
          }
        }
        """

    @api-QtQuick.Controls.AbstractButton.icon.color
    Scenario Outline: The icon of the button "<button>" is drawn in <colour>
      Then the pixel at 50, <y> of the window is "<colour>"

      Examples:
        | button  | y   | colour  |
        | plain   | 20  | #26282a |
        | own     | 70  | #00ff00 |
        | asIs    | 120 | #ff0000 |
        | bright  | 170 | #ffffff |
        | checked | 220 | #ffffff |

    Scenario: A button that was given no colour for its icon says none
      Then the property "icon.color" of "plain" is "#00000000"

    Scenario: A colour given later is the colour of the icon
      When the property "icon.color" of "plain" is set to "#0000ff"
      Then the pixel at 50, 20 of the window is "#0000ff"

    Scenario: The icon follows the colour of the text
      When the property "highlighted" of "plain" is set to true
      Then the pixel at 50, 20 of the window is "#ffffff"

  Rule: The display says whether the icon, the text or both are shown

    Background:
      Given a 16 by 16 image file "red.png" filled with "#ff0000"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          id: root; objectName: "root"
          width: 300; height: 300; color: "white"
          property bool mirror: false
          property real wider: both.implicitContentWidth - text.implicitContentWidth
          property real taller: both.implicitContentHeight - text.implicitContentHeight
          property real widest: Math.max(16, text.implicitContentWidth)
          property bool asWideAsTheWider: both.implicitContentWidth === widest
          Button { id: text; objectName: "text"; text: "on" }
          Button {
            id: both; objectName: "both"; y: 100
            text: "on"; icon.source: "red.png"; icon.color: "#00ff00"
            LayoutMirroring.enabled: root.mirror
          }
        }
        """

    @api-QtQuick.Controls.AbstractButton.display
    Scenario: The icon is beside the text unless told otherwise
      Then the property "display" of "both" is 2
      And the property "wider" of "root" is 22
      And the property "taller" of "root" is 2

    Scenario: The space between the icon and the text is the spacing
      When the property "spacing" of "both" is set to 10
      Then the property "wider" of "root" is 26

    Scenario: The icon comes before the text
      Then the pixel at 38, 113 of the window is "#00ff00"
      And the pixel at 61, 113 of the window is "#e0e0e0"

    Scenario: The icon comes after the text in a mirrored button
      When the property "mirror" of "root" is set to true
      Then the pixel at 61, 113 of the window is "#00ff00"
      And the pixel at 38, 113 of the window is "#e0e0e0"

    Scenario: The icon alone
      When the property "display" of "both" is set to 0
      Then the property "implicitContentWidth" of "both" is 16
      And the property "implicitContentHeight" of "both" is 16
      And the pixel at 50, 120 of the window is "#00ff00"

    Scenario: The text alone
      When the property "display" of "both" is set to 1
      Then the property "wider" of "root" is 0
      And the property "taller" of "root" is 0
      And the pixel at 38, 113 of the window is "#e0e0e0"

    Scenario: The text under the icon
      When the property "display" of "both" is set to 3
      Then the property "asWideAsTheWider" of "root" is true
      And the property "taller" of "root" is 22
      And the pixel at 50, 110 of the window is "#00ff00"
      And the pixel at 50, 122 of the window is "#e0e0e0"

    Scenario: A button without text shows its icon whatever the display
      When the property "text" of "both" is set to ""
      And the property "display" of "both" is set to 3
      Then the property "implicitContentWidth" of "both" is 16
      And the property "implicitContentHeight" of "both" is 16

  Rule: An icon is named by a theme or by a file

    Background:
      Given a 16 by 16 image file "red.png" filled with "#ff0000"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 300; color: "white"
          Button { objectName: "named"; icon.name: "no-such-icon" }
          Button {
            objectName: "either"; y: 50
            icon.name: "no-such-icon"; icon.source: "red.png"
          }
        }
        """

    @api-QtQuick.Controls.AbstractButton.icon.name
    Scenario: A name no theme has shows nothing
      Then the property "icon.name" of "named" is "no-such-icon"
      And the property "implicitContentWidth" of "named" is 0
      And the pixel at 50, 20 of the window is "#e0e0e0"
      And the log has no warnings

    Scenario: The file is shown when no theme has the name
      Then the property "implicitContentWidth" of "either" is 16
      And the pixel at 50, 70 of the window is "#26282a"

  Rule: Every kind of button shows its icon

    Background:
      Given a 16 by 16 image file "red.png" filled with "#ff0000"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 300; color: "white"
          ToolButton { objectName: "tool"; icon.source: "red.png" }
          ItemDelegate { objectName: "row"; y: 50; width: 200; text: "on"; icon.source: "red.png" }
          MenuItem { objectName: "item"; y: 100; text: "on"; icon.source: "red.png" }
          MenuItem {
            objectName: "checkable"; y: 150
            text: "on"; icon.source: "red.png"; checkable: true
          }
          MenuItem { objectName: "bare"; y: 200; icon.source: "red.png" }
          CheckBox { objectName: "box"; y: 250; icon.source: "red.png" }
          Button {
            objectName: "acting"; x: 200
            action: Action { icon.source: "red.png"; icon.color: "#0000ff" }
          }
        }
        """

    Scenario: A tool button with an icon stays square
      Then the property "implicitWidth" of "tool" is 40
      And the property "implicitContentWidth" of "tool" is 16
      And the pixel at 20, 20 of the window is "#26282a"
      And the pixel at 11, 20 of the window is within 1 of "#f0f0f0"

    Scenario: The icon of a row is at its start, in the colour of its text
      Then the pixel at 20, 72 of the window is "#353637"
      And the pixel at 11, 72 of the window is "#ffffff"
      And the pixel at 29, 65 of the window is "#ffffff"

    Scenario: The icon of a row that shows nothing else is in its middle
      When the property "display" of "row" is set to 0
      Then the pixel at 100, 70 of the window is "#353637"
      And the pixel at 20, 70 of the window is "#ffffff"

    Scenario: The icon of a menu item is at its start
      Then the pixel at 14, 120 of the window is "#26282a"
      And the pixel at 23, 113 of the window is "#ffffff"

    Scenario: The icon of a menu item that shows nothing else stays at its start
      When the property "display" of "item" is set to 0
      Then the pixel at 14, 120 of the window is "#26282a"
      And the pixel at 100, 120 of the window is "#ffffff"

    Scenario: The icon of a menu item that can be checked is after the room for the mark
      Then the pixel at 14, 170 of the window is "#ffffff"
      And the pixel at 48, 170 of the window is "#26282a"

    Scenario: A menu item without text is as wide as its icon
      Then the property "implicitContentWidth" of "bare" is 16

    Scenario: A check box shows no icon
      # The room for its indicator, as without an icon.
      Then the property "implicitContentWidth" of "box" is 34
      And the pixel at 50, 265 of the window is "#ffffff"

    @api-QtQuick.Controls.AbstractButton.action
    Scenario: A button shows the icon of its action
      Then the property "icon.color" of "acting" is "#0000ff"
      And the property "implicitContentWidth" of "acting" is 16
      And the pixel at 250, 20 of the window is "#0000ff"
