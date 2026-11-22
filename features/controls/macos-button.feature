# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-macos.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html
# Qt ships the macOS style only on macOS, so there is no Qt on this
# platform to hold these scenarios to; Ljós has the style everywhere.

@ljos-only
Feature: A button of the macOS style
  `import QtQuick.Controls.macOS` gives the controls of Qt's macOS
  style. A flat, icon-only button is the shape of a toolbar item: no
  plate of its own, its text its tooltip and accessible name. A name
  in `icon.name` asks for a system symbol; Ljós has no icon theme and
  no SF Symbols, so a name shows nothing, as in every style. A control
  the style does not draw yet is Basic's, where Qt falls back to
  Fusion for the controls its macOS style does not support.

  Rule: A toolbar button

    Background:
      Given a 16 by 16 image file "red.png" filled with "#ff0000"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.macOS
        Item {
          width: 300; height: 200
          Button {
            objectName: "toolbar"
            text: "Sidebar"
            Accessible.name: text
            ToolTip.delay: 500
            ToolTip.text: text
            ToolTip.visible: hovered || visualFocus
            display: AbstractButton.IconOnly
            flat: true
            icon.name: "sidebar.left"
            icon.source: "red.png"
            icon.width: 20
            icon.height: 20
          }
          Button { objectName: "plain"; y: 60 }
          CheckBox { objectName: "box"; y: 120; text: "On" }
        }
        """

    Scenario Outline: The button keeps what it was given
      Then the property "<property>" of "toolbar" is <value>

      Examples:
        | property   | value |
        | flat       | true  |
        | display    | 0     |
        | icon.width | 20    |
        | icon.height | 20   |

    Scenario: The icon alone is the content
      Then the property "implicitContentWidth" of "toolbar" is 16
      And the property "implicitContentHeight" of "toolbar" is 16

    Scenario: A flat button shows no plate
      Then the property "background.visible" of "toolbar" is false

    Scenario Outline: The button's text is its tooltip and accessible name
      Then the property "<property>" of "toolbar" is "Sidebar"

      Examples:
        | property         |
        | Accessible.name  |
        | ToolTip.text     |

    @api-QtQuick.Controls.Control.visualFocus
    Scenario: Tab shows a flat button's focus ring without a plate
      When the user presses "Tab"
      Then the property "visualFocus" of "toolbar" is true
      And the property "background.visible" of "toolbar" is true
      And the property "background.border.width" of "toolbar" is 2
      And the property "background.color" of "toolbar" is "#00000000"

    Scenario Outline: A button without text has the size of its background
      Then the property "<property>" of "plain" is <value>

      Examples:
        | property                 | value |
        | implicitBackgroundWidth  | 64    |
        | implicitBackgroundHeight | 21    |
        | implicitWidth            | 64    |
        | implicitHeight           | 21    |
        | background.radius        | 6     |
        | background.visible       | true  |

    Scenario: A control the style does not draw yet still resolves
      Then the property "text" of "box" is "On"
      And the log has no warnings
