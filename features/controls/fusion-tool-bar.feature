# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-toolbutton.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-toolbar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-toolseparator.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.ToolButton @api-QtQuick.Controls.ToolBar
@api-QtQuick.Controls.ToolSeparator
Feature: The Fusion style's tool bar
  A Fusion tool button is as small as its text or icon and shows its
  button only while it is pressed, checked or highlighted. A tool bar is
  as tall as one row of them, with a light line along its top and a dark
  one along its bottom; a separator is a short groove.

  Background:
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """
    And the file "Tools.qml":
      """qml
      import QtQuick
      import QtQuick.Controls
      Column {
        ToolButton { objectName: "plain"; text: "Ab" }
        ToolButton { objectName: "empty" }
        ToolButton { objectName: "down"; text: "Ab"; down: true }
        ToolButton { objectName: "checked"; text: "Ab"; checkable: true; checked: true }
        ToolButton { objectName: "highlighted"; text: "Ab"; highlighted: true }
        ToolButton { objectName: "off"; text: "Ab"; enabled: false }
        ToolButton {
          objectName: "offChecked"; text: "Ab"
          enabled: false; checkable: true; checked: true
        }
        ToolBar { objectName: "bar" }
        ToolBar { objectName: "footer"; position: ToolBar.Footer }
        ToolBar { objectName: "row"; ToolButton { text: "Ab" } }
        ToolSeparator { objectName: "upright" }
        ToolSeparator { objectName: "across"; orientation: Qt.Horizontal }
      }
      """

  Scenario Outline: A tool button's <property> when it is <object>
    Given the QML:
      """qml
      import QtQuick
      Tools {}
      """
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object      | property                  | value       |
      | plain       | implicitHeight            | 26          |
      | plain       | padding                   | 6           |
      | plain       | spacing                   | 6           |
      | plain       | icon.width                | 16          |
      | plain       | icon.height               | 16          |
      | plain       | background.implicitWidth  | 20          |
      | plain       | background.implicitHeight | 20          |
      | plain       | background.visible        | false       |
      | plain       | background.radius         | 2           |
      | plain       | background.color          | "#e8e8e8"   |
      | plain       | background.border.color   | "#ababab"   |
      | plain       | contentItem.color         | "#000000"   |
      | empty       | implicitWidth             | 20          |
      | empty       | implicitHeight            | 20          |
      | down        | background.visible        | true        |
      | down        | background.color          | "#d3d3d3"   |
      | checked     | background.visible        | true        |
      | checked     | background.color          | "#d3d3d3"   |
      | highlighted | background.visible        | true        |
      | highlighted | background.border.color   | "#26709e"   |
      | off         | background.visible        | false       |
      | off         | contentItem.color         | "#bebebe"   |
      | offChecked  | background.visible        | true        |

  Scenario Outline: A tool bar's <property> when it is <object>
    Given the QML:
      """qml
      import QtQuick
      Tools {}
      """
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property                       | value       |
      | bar    | implicitWidth                  | 12          |
      | bar    | implicitHeight                 | 26          |
      | bar    | leftPadding                    | 6           |
      | bar    | rightPadding                   | 6           |
      | bar    | topPadding                     | 0           |
      | bar    | bottomPadding                  | 1           |
      | bar    | background.implicitHeight      | 26          |
      | bar    | background.children.0.y        | 0           |
      | bar    | background.children.0.height   | 1           |
      | bar    | background.children.0.color    | "#5affffff" |
      | bar    | background.children.1.y        | 25          |
      | bar    | background.children.1.height   | 1           |
      | bar    | background.children.1.color    | "#3c000000" |
      | footer | topPadding                     | 1           |
      | footer | bottomPadding                  | 0           |
      | row    | implicitHeight                 | 27          |

  Scenario Outline: A tool separator's <property> when it is <object>
    Given the QML:
      """qml
      import QtQuick
      Tools {}
      """
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object  | property                     | value     |
      | upright | implicitWidth                | 14        |
      | upright | implicitHeight               | 12        |
      | upright | leftPadding                  | 6         |
      | upright | topPadding                   | 2         |
      | upright | contentItem.implicitWidth    | 2         |
      | upright | contentItem.implicitHeight   | 8         |
      | upright | contentItem.color            | "#d9d9d9" |
      | upright | contentItem.children.0.x     | 1         |
      | upright | contentItem.children.0.width | 1         |
      | upright | contentItem.children.0.color | "#ffffff" |
      | across  | implicitWidth                | 12        |
      | across  | implicitHeight               | 14        |
      | across  | leftPadding                  | 2         |
      | across  | topPadding                   | 6         |
