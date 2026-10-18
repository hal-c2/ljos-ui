# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dialog.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dialogbuttonbox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tooltip.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.Popup @api-QtQuick.Controls.Dialog
@api-QtQuick.Controls.DialogButtonBox @api-QtQuick.Controls.ToolTip
Feature: The Fusion style's popups
  A Fusion popup or dialog is the window's colour in a rounded frame of
  the mid colour, with half the room around its content that Basic's has.
  A dialog's title and buttons sit on it with nothing of their own behind
  them; a tool tip is framed in the colour of its text.

  Background:
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """
    And the file "Popups.qml":
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        width: 300; height: 300
        Popup { objectName: "popup"; visible: true; width: 100; height: 50 }
        Dialog {
          objectName: "dialog"; visible: true; title: "T"
          standardButtons: Dialog.Ok
        }
        DialogButtonBox {
          objectName: "box"
          standardButtons: DialogButtonBox.Ok | DialogButtonBox.Cancel
        }
        ToolTip { objectName: "tip"; visible: true; text: "Ab" }
      }
      """

  Scenario Outline: The <property> of a <object>
    Given the QML:
      """qml
      import QtQuick
      Popups {}
      """
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property                   | value       |
      | popup  | implicitWidth              | 12          |
      | popup  | padding                    | 6           |
      | popup  | background.color           | "#efefef"   |
      | popup  | background.border.color    | "#b8b8b8"   |
      | popup  | background.radius          | 2           |
      | dialog | padding                    | 6           |
      | dialog | background.color           | "#efefef"   |
      | dialog | background.border.color    | "#b8b8b8"   |
      | dialog | background.radius          | 2           |
      | dialog | header.implicitHeight      | 26          |
      | dialog | header.padding             | 6           |
      | dialog | header.font.bold           | true        |
      | dialog | header.color               | "#000000"   |
      | dialog | header.background.color    | "#00000000" |
      | dialog | footer.implicitHeight      | 36          |
      | dialog | footer.padding             | 6           |
      | dialog | footer.background.color    | "#00000000" |
      | box    | implicitWidth              | 178         |
      | box    | implicitHeight             | 36          |
      | box    | padding                    | 6           |
      | box    | spacing                    | 6           |
      | tip    | padding                    | 6           |
      | tip    | margins                    | 6           |
      | tip    | background.color           | "#ffffdc"   |
      | tip    | background.border.color    | "#000000"   |
      | tip    | contentItem.color          | "#000000"   |
