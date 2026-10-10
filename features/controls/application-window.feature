# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-applicationwindow.html
#   https://doc.qt.io/qt-6/qml-qtquick-window.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: ApplicationWindow
  An application window is a window with a header above its content and a
  footer below it. Its font and palette are those of every control in it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        objectName: "win"
        width: 300; height: 200
        visible: true
        title: "Demo"
        font.pixelSize: 20
        palette.button: "#aa0000"
        header: ToolBar { objectName: "head" }
        footer: Rectangle { objectName: "foot"; implicitHeight: 15 }
        Button { objectName: "b" }
        Label { objectName: "l"; y: 50 }
        Pane {
          y: 80
          Button { objectName: "deep" }
        }
      }
      """

  @api-QtQuick.Controls.ApplicationWindow.header @api-QtQuick.Controls.ApplicationWindow.footer
  Scenario Outline: The header is at the top and the footer at the bottom, as wide as the window
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property | value |
      | head   | x        | 0     |
      | head   | y        | 0     |
      | head   | width    | 300   |
      | head   | height   | 40    |
      | foot   | x        | 0     |
      | foot   | y        | 185   |
      | foot   | width    | 300   |
      | foot   | height   | 15    |

  @api-QtQuick.Controls.ApplicationWindow.contentItem
  Scenario Outline: The content lies between them
    Then the property "<property>" of "win" is <value>

    Examples:
      | property              | value     |
      | contentItem.x         | 0         |
      | contentItem.y         | 40        |
      | contentItem.width     | 300       |
      | contentItem.height    | 145       |
      | title                 | "Demo"    |
      | color                 | "#ffffff" |
      | background            | null      |
      | header.objectName     | "head"    |
      | footer.objectName     | "foot"    |

  Scenario: What is put into the window becomes a child of its content item
    Then the property "parent.y" of "b" is 40
    And the property "parent.width" of "b" is 300

  Scenario: The bars follow the size of the window
    When the property "width" of "win" is set to 400
    And the property "height" of "win" is set to 300
    And 100 milliseconds pass
    Then the property "width" of "head" is 400
    And the property "y" of "foot" is 285
    And the property "contentItem.height" of "win" is 245

  Scenario: A hidden footer gives its room to the content
    When the property "visible" of "foot" is set to false
    Then the property "contentItem.height" of "win" is 160

  @api-QtQuick.Controls.ApplicationWindow.font
  Scenario: Every control in the window takes the font of the window
    Then the property "font.pixelSize" of "b" is 20
    And the property "font.pixelSize" of "l" is 20
    And the property "font.pixelSize" of "deep" is 20
    And the property "font.pixelSize" of "head" is 20
    When the property "font.pixelSize" of "win" is set to 26
    Then the property "font.pixelSize" of "deep" is 26

  @api-QtQuick.Controls.ApplicationWindow.palette
  Scenario: Every control in the window takes the palette of the window
    Then the property "palette.button" of "b" is "#aa0000"
    And the property "palette.button" of "deep" is "#aa0000"
    And the property "background.color" of "head" is "#aa0000"
    When the property "palette.button" of "win" is set to "#00aa00"
    Then the property "background.color" of "deep" is "#00aa00"

  Scenario: The window is filled with the window colour of its palette
    When the property "palette.window" of "win" is set to "#101010"
    Then the property "color" of "win" is "#101010"

  Scenario: A plain window passes its palette on as well
    Given the file "Plain.qml":
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Window {
        objectName: "w"
        width: 320; height: 240
        visible: true
        palette.windowText: "#aa00aa"
        Label { objectName: "inWindow" }
        Rectangle { objectName: "fill"; anchors.fill: parent }
      }
      """
    When "Plain.qml" is loaded
    And 100 milliseconds pass
    Then the property "color" of "inWindow" is "#aa00aa"
    And the property "width" of "fill" is 320
    And the property "height" of "fill" is 240
    And the property "contentItem.width" of "w" is 320
