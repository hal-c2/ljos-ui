# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-stylehints.html
#   https://doc.qt.io/qt-6/qml-qtquick-application.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Style hints
  `Application.styleHints` and `Qt.styleHints` are one object that tells
  how the platform wants an interface to behave.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool same: Application.styleHints === Qt.styleHints
        property int scheme: Application.styleHints.colorScheme
        property bool light: Application.styleHints.colorScheme === Qt.Light
        property int dragDistance: Application.styleHints.startDragDistance
        property int flash: Application.styleHints.cursorFlashTime
        property int keyboardInterval: Application.styleHints.keyboardInputInterval
        property int pressAndHold: Application.styleHints.mousePressAndHoldInterval
        property bool hover: Application.styleHints.useHoverEffects
        property string mask: Application.styleHints.passwordMaskCharacter
        property int wheelLines: Application.styleHints.wheelScrollLines
      }
      """

  @api-QtQuick.Application.styleHints
  Scenario: Both names are the same object
    Then the property "same" of "root" is true

  @api-QtQuick.StyleHints.colorScheme
  Scenario: Light is what the system says without a preference
    Then the property "scheme" of "root" is 1
    And the property "light" of "root" is true

  @api-QtQuick.StyleHints.startDragDistance @api-QtQuick.StyleHints.cursorFlashTime
  @api-QtQuick.StyleHints.keyboardInputInterval @api-QtQuick.StyleHints.mousePressAndHoldInterval
  @api-QtQuick.StyleHints.useHoverEffects @api-QtQuick.StyleHints.wheelScrollLines
  Scenario Outline: The constants are a desktop's
    Then the property "<property>" of "root" is <value>

    Examples:
      | property         | value |
      | dragDistance     | 8     |
      | flash            | 1200  |
      | keyboardInterval | 400   |
      | pressAndHold     | 500   |
      | hover            | true  |
      | wheelLines       | 3     |

  @api-QtQuick.StyleHints.passwordMaskCharacter
  Scenario: The password mask is a bullet
    Then the property "mask" of "root" is "•"
