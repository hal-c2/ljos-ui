# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-applicationwindow.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubar.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The menu bar of an application window
  The menu bar of an application window lies across its top, above the
  header. The content begins below the two.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        objectName: "win"
        width: 400; height: 400
        visible: true
        menuBar: MenuBar {
          objectName: "bar"
          Menu { title: "File"; MenuItem { text: "New" } }
        }
        header: ToolBar { objectName: "head" }
      }
      """

  @api-QtQuick.Controls.ApplicationWindow.menuBar
  Scenario Outline: The <property> of the <object> is <value>
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property           | value |
      | bar    | x                  | 0     |
      | bar    | y                  | 0     |
      | bar    | width              | 400   |
      | bar    | height             | 40    |
      | bar    | z                  | 2     |
      | bar    | count              | 1     |
      | head   | y                  | 40    |
      | head   | width              | 400   |
      | head   | z                  | 1     |
      | win    | menuBar.objectName | "bar" |
      | win    | contentItem.y      | 80    |
      | win    | contentItem.height | 320   |

  @api-QtQuick.Controls.ApplicationWindow.menuBar
  Scenario: A hidden menu bar gives its room to the header
    When the property "visible" of "bar" is set to false
    Then the property "y" of "head" is 0
    And the property "contentItem.y" of "win" is 40
    And the property "contentItem.height" of "win" is 360

  @api-QtQuick.Controls.ApplicationWindow.menuBar
  Scenario: A taller menu bar pushes the header down
    When the property "height" of "bar" is set to 60
    Then the property "y" of "head" is 60
    And the property "contentItem.y" of "win" is 100
