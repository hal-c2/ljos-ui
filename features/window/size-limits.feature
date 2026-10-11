# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The smallest and the largest size of a window
  A window is never smaller than its minimum size nor larger than its
  maximum size: setting a limit the window is outside of resizes it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "w"
        width: 300; height: 200
        visible: true
      }
      """

  @api-QtQuick.Window.minimumWidth @api-QtQuick.Window.minimumHeight
  Scenario Outline: A minimum above the size grows the window
    When the property "<limit>" of "w" is set to 400
    Then the property "<size>" of "w" is 400

    Examples:
      | limit         | size   |
      | minimumWidth  | width  |
      | minimumHeight | height |

  @api-QtQuick.Window.maximumWidth @api-QtQuick.Window.maximumHeight
  Scenario Outline: A maximum below the size shrinks the window
    When the property "<limit>" of "w" is set to 150
    Then the property "<size>" of "w" is 150

    Examples:
      | limit         | size   |
      | maximumWidth  | width  |
      | maximumHeight | height |

  @api-QtQuick.Window.minimumWidth @api-QtQuick.Window.maximumWidth
  Scenario Outline: A limit the window is inside of leaves its size alone
    When the property "<limit>" of "w" is set to <value>
    Then the property "width" of "w" is 300
    And the property "<limit>" of "w" is <value>

    Examples:
      | limit        | value |
      | minimumWidth | 100   |
      | maximumWidth | 500   |
