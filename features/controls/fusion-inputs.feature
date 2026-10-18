# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-combobox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-spinbox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-textarea.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-itemdelegate.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dial.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.ComboBox @api-QtQuick.Controls.SpinBox
@api-QtQuick.Controls.TextArea @api-QtQuick.Controls.ItemDelegate
@api-QtQuick.Controls.Dial
Feature: The Fusion style's inputs
  Fusion's combo box and spin box are as tall as its button, with a
  narrow strip at the right for the arrow or the two steps. A text area
  is framed like a field, a row of a list is as tall as a line of text,
  and a dial is smaller than Basic's.

  Background:
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """
    And the file "Inputs.qml":
      """qml
      import QtQuick
      import QtQuick.Controls
      Column {
        ComboBox { objectName: "combo"; model: ["Ab", "Cd"] }
        SpinBox { objectName: "spin" }
        TextArea { objectName: "area"; text: "Ab" }
        ItemDelegate { objectName: "row"; text: "Ab" }
        ItemDelegate { objectName: "chosenRow"; text: "Ab"; highlighted: true }
        ItemDelegate { objectName: "pressedRow"; text: "Ab"; down: true }
        Dial { objectName: "dial" }
      }
      """

  Scenario Outline: The <property> of the <object>
    Given the QML:
      """qml
      import QtQuick
      Inputs {}
      """
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object     | property                    | value     |
      | combo      | implicitWidth               | 120       |
      | combo      | implicitHeight              | 24        |
      | combo      | leftPadding                 | 0         |
      | combo      | rightPadding                | 20        |
      | combo      | background.color            | "#e8e8e8" |
      | combo      | background.border.color     | "#ababab" |
      | combo      | background.radius           | 2         |
      | combo      | indicator.implicitWidth     | 10        |
      | combo      | indicator.implicitHeight    | 6         |
      | combo      | indicator.x                 | 100       |
      | combo      | contentItem.leftPadding     | 4         |
      | combo      | contentItem.color           | "#000000" |
      | spin       | implicitWidth               | 120       |
      | spin       | implicitHeight              | 24        |
      | spin       | topPadding                  | 4         |
      | spin       | leftPadding                 | 4         |
      | spin       | rightPadding                | 20        |
      | spin       | background.color            | "#ffffff" |
      | spin       | background.border.color     | "#ababab" |
      | spin       | background.radius           | 2         |
      | spin       | up.indicator.implicitWidth  | 16        |
      | spin       | up.indicator.implicitHeight | 10        |
      | spin       | up.indicator.x              | 103       |
      | spin       | up.indicator.y              | 1         |
      | spin       | down.indicator.x            | 103       |
      | spin       | down.indicator.y            | 12        |
      | area       | implicitWidth               | 120       |
      | area       | leftPadding                 | 10        |
      | area       | topPadding                  | 6         |
      | area       | background.visible          | true      |
      | area       | background.color            | "#ffffff" |
      | area       | background.border.color     | "#ababab" |
      | area       | background.radius           | 2         |
      | row        | implicitHeight              | 26        |
      | row        | padding                     | 6         |
      | row        | spacing                     | 6         |
      | row        | icon.width                  | 16        |
      | row        | background.implicitHeight   | 20        |
      | row        | background.visible          | true      |
      | row        | background.color            | "#ffffff" |
      | row        | contentItem.color           | "#000000" |
      | chosenRow  | background.color            | "#308cc6" |
      | chosenRow  | contentItem.color           | "#ffffff" |
      | pressedRow | background.color            | "#dbdbdb" |
      | dial       | implicitWidth               | 100       |
      | dial       | implicitHeight              | 100       |
