# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Declaring how a window is shown
  A window whose `visibility` names a shown state is shown in that state
  without `visible` being set. The automatic visibility leaves a window
  that was never shown hidden.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        Window {
          objectName: "max"
          width: 300; height: 200
          visibility: Window.Maximized
        }
        Window {
          objectName: "auto"
          width: 300; height: 200
          visibility: Window.AutomaticVisibility
        }
        Window {
          objectName: "shown"
          width: 300; height: 200
          visible: true
        }
      }
      """

  @api-QtQuick.Window.visibility
  Scenario: A window declared maximized is shown
    Then the property "visible" of "max" is true
    And the property "visibility" of "max" is 4

  @api-QtQuick.Window.visibility
  Scenario: The automatic visibility does not show a window
    Then the property "visible" of "auto" is false
    And the property "visibility" of "auto" is 0

  @api-QtQuick.Window.visible
  Scenario: A window declared visible is windowed at its own size
    Then the property "visibility" of "shown" is 2
    And the property "width" of "shown" is 300
    And the property "height" of "shown" is 200
