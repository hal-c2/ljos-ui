# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tabbar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tabbutton.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.TabBar @api-QtQuick.Controls.TabButton
Feature: The Fusion style's tab bar
  Fusion's tabs are as tall as a line of text and a little more, overlap
  by their border, and draw nothing behind them. The current tab is the
  full height of the bar; the others start a little lower.

  Background:
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """
    And the file "Tabs.qml":
      """qml
      import QtQuick
      import QtQuick.Controls
      Column {
        TabBar {
          objectName: "bar"
          TabButton { objectName: "current"; text: "Ab" }
          TabButton { objectName: "other"; text: "Cd" }
        }
        TabButton { objectName: "alone"; text: "Ab" }
        TabButton { objectName: "off"; text: "Ab"; enabled: false }
      }
      """

  Scenario Outline: The <property> of the <object> tab
    Given the QML:
      """qml
      import QtQuick
      Tabs {}
      """
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object  | property                | value     |
      | bar     | implicitHeight          | 21        |
      | bar     | spacing                 | -1        |
      | current | height                  | 21        |
      | current | background.y            | 0         |
      | current | background.height       | 21        |
      | current | background.border.color | "#bcbcbc" |
      | current | contentItem.color       | "#000000" |
      | other   | background.y            | 2         |
      | other   | background.height       | 19        |
      | other   | contentItem.color       | "#000000" |
      | alone   | implicitHeight          | 21        |
      | alone   | topPadding              | 2         |
      | alone   | leftPadding             | 4         |
      | off     | contentItem.color       | "#bebebe" |
