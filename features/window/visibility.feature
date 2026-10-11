# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Showing and hiding a window
  `visible` says whether a window is on the screen and `visibility` how:
  hidden (0), windowed (2), minimized (3), maximized (4) or over the whole
  screen (5). Each follows the other.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "w"
        width: 300; height: 200
      }
      """

  @api-QtQuick.Window.visible @api-QtQuick.Window.visibility
  Scenario: A window that is made visible is windowed
    When the property "visible" of "w" is set to true
    Then the property "visibility" of "w" is 2

  @api-QtQuick.Window.show
  Scenario: show() makes the window visible
    When the method "show" of "w" is called
    Then the property "visible" of "w" is true
    And the property "visibility" of "w" is 2

  @api-QtQuick.Window.hide
  Scenario: hide() takes the window off the screen
    When the method "show" of "w" is called
    And the method "hide" of "w" is called
    Then the property "visible" of "w" is false
    And the property "visibility" of "w" is 0

  @api-QtQuick.Window.showMaximized @api-QtQuick.Window.showFullScreen @api-QtQuick.Window.showMinimized
  Scenario Outline: A window can be shown maximized, over the whole screen or minimized
    When the method "<method>" of "w" is called
    Then the property "visibility" of "w" is <visibility>
    And the property "visible" of "w" is true

    Examples:
      | method         | visibility |
      | showMaximized  | 4          |
      | showFullScreen | 5          |
      | showMinimized  | 3          |

  @api-QtQuick.Window.showNormal
  Scenario Outline: showNormal() gives a window its own size back
    When the method "show" of "w" is called
    And the method "<method>" of "w" is called
    And the method "showNormal" of "w" is called
    Then the property "visibility" of "w" is 2
    And the property "width" of "w" is 300
    And the property "height" of "w" is 200

    Examples:
      | method         |
      | showMaximized  |
      | showFullScreen |

  @api-QtQuick.Window.visibility
  Scenario: Setting the visibility to a shown state shows the window
    When the property "visibility" of "w" is set to 4
    Then the property "visible" of "w" is true

  @api-QtQuick.Window.visibility
  Scenario: Setting the visibility to hidden hides the window
    When the method "show" of "w" is called
    And the property "visibility" of "w" is set to 0
    Then the property "visible" of "w" is false

  @api-QtQuick.Window.visibility
  Scenario: A maximized window that was hidden comes back maximized
    When the method "showMaximized" of "w" is called
    And the property "visible" of "w" is set to false
    Then the property "visibility" of "w" is 0
    When the property "visible" of "w" is set to true
    Then the property "visibility" of "w" is 4
