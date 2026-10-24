# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-palette.html
#   https://doc.qt.io/qt-6/qml-qtquick-colorgroup.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-button.html
# Verified on Qt 6.11.2 (qml6, offscreen), in the Basic and Fusion styles:
# colours read from the controls, and from a picture of the window.

@qt-parity @api-QtQuick.Palette @api-QtQuick.ColorGroup
Feature: How a disabled button looks under a palette of the application's
  A disabled button is drawn in the colours of the palette's disabled
  group. A colour set on the palette itself is set in every group, the
  disabled one too, so a window that sets its buttons' colours and
  nothing for the disabled group has disabled buttons that look as the
  others do, in qml6 as here. An application that wants them to look
  disabled sets the disabled group's colours as well.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 300; height: 180; color: "white"
        Button { objectName: "plain"; text: "Next"; width: 100; height: 40; enabled: false }
        Item {
          y: 60
          palette.button: "#e2e5ea"
          palette.buttonText: "#1d2027"
          Button { objectName: "set"; text: "Next"; width: 100; height: 40; enabled: false }
          Button { objectName: "on"; text: "Next"; x: 150; width: 100; height: 40 }
        }
        Item {
          y: 120
          palette.button: "#e2e5ea"
          palette.buttonText: "#1d2027"
          palette.disabled.button: "#f0f1f3"
          palette.disabled.buttonText: "#9aa0aa"
          Button { objectName: "grouped"; text: "Next"; width: 100; height: 40; enabled: false }
        }
      }
      """

  Scenario: A disabled button has the style's disabled colours
    Then the property "contentItem.color" of "plain" is "#4d26282a"
    And the property "background.color" of "plain" is "#e0e0e0"

  Scenario: A colour set on the palette is the disabled button's colour too
    Then the property "contentItem.color" of "set" is "#1d2027"
    And the property "contentItem.color" of "on" is "#1d2027"
    And the property "background.color" of "set" is "#e2e5ea"
    And the pixel at 5, 65 of the window is "#e2e5ea"
    And the pixel at 155, 65 of the window is "#e2e5ea"

  Scenario: The disabled group's colours are the ones a disabled button has
    Then the property "contentItem.color" of "grouped" is "#9aa0aa"
    And the property "background.color" of "grouped" is "#f0f1f3"
    And the pixel at 5, 125 of the window is "#f0f1f3"

  Scenario: Fusion draws a disabled button in the colours set on the palette too
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Fusion
      Rectangle {
        width: 300; height: 60; color: "white"
        Button { objectName: "plain"; text: "Next"; width: 100; height: 40; enabled: false }
        Item {
          x: 150
          palette.button: "#e2e5ea"
          palette.buttonText: "#1d2027"
          Button { objectName: "set"; text: "Next"; width: 100; height: 40; enabled: false }
        }
      }
      """
    Then the property "contentItem.color" of "plain" is "#bebebe"
    And the property "contentItem.color" of "set" is "#1d2027"
