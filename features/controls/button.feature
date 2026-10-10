# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-button.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Button
  A push button of the Basic style: a flat grey rectangle with a centred
  text. Its parts can be replaced by the application.

  Rule: The Basic style

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 200
          Button {
            objectName: "b"
            property int clicks: 0
            onClicked: clicks++
          }
        }
        """

    Scenario Outline: A button without text has the size of its background
      Then the property "<property>" of "b" is <value>

      Examples:
        | property                 | value |
        | implicitWidth            | 100   |
        | implicitHeight           | 40    |
        | width                    | 100   |
        | height                   | 40    |
        | implicitBackgroundWidth  | 100   |
        | implicitBackgroundHeight | 40    |
        | implicitContentWidth     | 0     |
        | implicitContentHeight    | 0     |
        | padding                  | 6     |
        | horizontalPadding        | 8     |
        | verticalPadding          | 6     |
        | leftPadding              | 8     |
        | rightPadding             | 8     |
        | topPadding               | 6     |
        | bottomPadding            | 6     |
        | spacing                  | 6     |
        | availableWidth           | 84    |
        | availableHeight          | 28    |

    @api-QtQuick.Controls.Button.flat @api-QtQuick.Controls.Button.highlighted
    Scenario Outline: A button starts out plain
      Then the property "<property>" of "b" is <value>

      Examples:
        | property      | value |
        | flat          | false |
        | highlighted   | false |
        | checkable     | false |
        | checked       | false |
        | autoExclusive | false |
        | icon.width    | 24    |
        | icon.height   | 24    |
        | display       | 2     |
        | hoverEnabled  | true  |
        | wheelEnabled  | false |

    Scenario Outline: The parts fill the button
      When the property "width" of "b" is set to 200
      And the property "height" of "b" is set to 60
      Then the property "<property>" of "b" is <value>

      Examples:
        | property           | value |
        | background.x       | 0     |
        | background.y       | 0     |
        | background.width   | 200   |
        | background.height  | 60    |
        | contentItem.x      | 8     |
        | contentItem.y      | 6     |
        | contentItem.width  | 184   |
        | contentItem.height | 48    |

    Scenario: A long text makes the button wider than its background
      When the property "text" of "b" is set to "A text that is wider than the background"
      Then the property "implicitWidth" of "b" is at least 101
      When the property "leftPadding" of "b" is set to 50
      And the property "rightPadding" of "b" is set to 30
      And the property "text" of "b" is bound to "implicitWidth - implicitContentWidth"
      Then the property "text" of "b" is "80"

    Scenario: A click is counted
      When the user touches 50, 20
      Then the property "clicks" of "b" is 1

    Scenario Outline: The colour of the background tells the state
      When the property "<state>" of "b" is set to true
      Then the property "background.color" of "b" is "<colour>"

      Examples:
        | state       | colour  |
        | enabled     | #e0e0e0 |
        | down        | #cfcfcf |
        | checked     | #353637 |
        | highlighted | #353637 |

    Scenario: A highlighted button held down is lighter
      When the property "highlighted" of "b" is set to true
      And the property "down" of "b" is set to true
      Then the property "background.color" of "b" is "#797a7a"

    Scenario: A flat button shows its background only while it is held down
      When the property "flat" of "b" is set to true
      Then the property "background.visible" of "b" is false
      When the property "down" of "b" is set to true
      Then the property "background.visible" of "b" is true

    Scenario Outline: A flat button that is checked or highlighted shows its background
      When the property "flat" of "b" is set to true
      And the property "<state>" of "b" is set to true
      Then the property "background.visible" of "b" is true

      Examples:
        | state       |
        | checked     |
        | highlighted |

  Rule: The application can replace the parts

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 200
          Button {
            objectName: "b"
            text: "Go"
            property int clicks: 0
            onClicked: clicks++
            background: Rectangle {
              objectName: "bg"
              implicitWidth: 160; implicitHeight: 56
              color: parent.down ? "#0d47a1" : "#1565c0"
            }
            contentItem: Text {
              objectName: "label"
              text: parent.text; color: "white"
              horizontalAlignment: Text.AlignHCenter
              verticalAlignment: Text.AlignVCenter
            }
          }
        }
        """

    Scenario Outline: The button takes its size from the new background
      Then the property "<property>" of "<object>" is <value>

      Examples:
        | object | property                 | value |
        | b      | implicitWidth            | 160   |
        | b      | implicitHeight           | 56    |
        | b      | implicitBackgroundWidth  | 160   |
        | b      | implicitBackgroundHeight | 56    |
        | bg     | width                    | 160   |
        | bg     | height                   | 56    |
        | bg     | z                        | -1    |
        | label  | x                        | 8     |
        | label  | y                        | 6     |
        | label  | width                    | 144   |
        | label  | height                   | 44    |
        | label  | text                     | "Go"  |

    Scenario: The new parts are the parts of the button
      Then the property "background.objectName" of "b" is "bg"
      And the property "contentItem.objectName" of "b" is "label"
      And the property "parent.objectName" of "bg" is "b"
      And the property "parent.objectName" of "label" is "b"

    Scenario: The new background sees the state of the button
      When the property "down" of "b" is set to true
      Then the property "color" of "bg" is "#0d47a1"

    Scenario: The button is still clicked
      When the user touches 80, 28
      Then the property "clicks" of "b" is 1

    Scenario: A content item larger than the background sets the size
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Button {
          objectName: "big"
          background: Rectangle { implicitWidth: 50; implicitHeight: 20 }
          contentItem: Item { implicitWidth: 200; implicitHeight: 70 }
        }
        """
      Then the property "implicitWidth" of "big" is 216
      And the property "implicitHeight" of "big" is 82

    Scenario: A button without a background is as large as its padded content
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Button {
          objectName: "bare"
          background: null
          contentItem: Item { implicitWidth: 30; implicitHeight: 10 }
        }
        """
      Then the property "implicitWidth" of "bare" is 46
      And the property "implicitHeight" of "bare" is 22

    Scenario: A component built on Button keeps the parts it gives
      Given the file "Fancy.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Button {
          background: Rectangle { objectName: "fancyBg"; implicitWidth: 120; implicitHeight: 48 }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          Fancy { objectName: "f"; text: "x" }
        }
        """
      Then the property "implicitWidth" of "f" is 120
      And the property "implicitHeight" of "f" is 48
      And the property "background.objectName" of "f" is "fancyBg"

    Scenario: The user of such a component can replace the part again
      Given the file "Fancy.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Button {
          background: Rectangle { objectName: "fancyBg"; implicitWidth: 120; implicitHeight: 48 }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          Fancy {
            objectName: "f"
            background: Rectangle { objectName: "mine"; implicitWidth: 70; implicitHeight: 30 }
          }
        }
        """
      Then the property "implicitWidth" of "f" is 70
      And the property "background.objectName" of "f" is "mine"
      And the property "width" of "mine" is 70
