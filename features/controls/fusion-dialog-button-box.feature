# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dialogbuttonbox.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.DialogButtonBox @api-QtQuick.Controls.Dialog
Feature: Fusion's dialog buttons sit to the right
  In the Fusion style a dialog's buttons are packed to the right however
  many there are, and the box asks only for the room its buttons take, so
  a dialog with one button is as wide as that button and the padding.

  Background:
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """

  Scenario Outline: A box of <buttons>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        DialogButtonBox { objectName: "box"; standardButtons: <buttons> }
      }
      """
    Then the property "implicitWidth" of "box" is <width>
    And the property "alignment" of "box" is 2

    Examples:
      | buttons                                          | width |
      | DialogButtonBox.Ok                               | 92    |
      | DialogButtonBox.Ok \| DialogButtonBox.Cancel      | 178   |

  Scenario: A dialog with one button is as wide as it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        Dialog { objectName: "dialog"; title: "Title"; standardButtons: Dialog.Ok; visible: true }
      }
      """
    Then the property "implicitWidth" of "dialog" is 92
