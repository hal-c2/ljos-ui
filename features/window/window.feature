# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Window
  A window is a top-level surface of the application. Until it is shown it
  has no size, no title and a white background.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "w"
      }
      """

  @api-QtQuick.Window.visible @api-QtQuick.Window.visibility @api-QtQuick.Window.title
  @api-QtQuick.Window.color @api-QtQuick.Window.flags @api-QtQuick.Window.modality
  Scenario Outline: A window starts hidden, empty and white
    Then the property "<property>" of "w" is <value>

    Examples:
      | property   | value     |
      | visible    | false     |
      | visibility | 0         |
      | title      | ""        |
      | width      | 0         |
      | height     | 0         |
      | x          | 0         |
      | y          | 0         |
      | color      | "#ffffff" |
      | active     | false     |
      | flags      | 1         |
      | modality   | 0         |
      | opacity    | 1         |

  @api-QtQuick.Window.minimumWidth @api-QtQuick.Window.minimumHeight
  @api-QtQuick.Window.maximumWidth @api-QtQuick.Window.maximumHeight
  Scenario Outline: A window may be as small as nothing and as large as the platform allows
    Then the property "<property>" of "w" is <value>

    Examples:
      | property      | value    |
      | minimumWidth  | 0        |
      | minimumHeight | 0        |
      | maximumWidth  | 16777215 |
      | maximumHeight | 16777215 |

  @api-QtQuick.Window.title @api-QtQuick.Window.color @api-QtQuick.Window.x @api-QtQuick.Window.y
  @api-QtQuick.Window.flags @api-QtQuick.Window.modality @api-QtQuick.Window.opacity
  Scenario Outline: What is written to a window is read back
    When the property "<property>" of "w" is set to <written>
    Then the property "<property>" of "w" is <read>

    Examples:
      | property | written  | read      |
      | title    | "Report" | "Report"  |
      | color    | "red"    | "#ff0000" |
      | x        | 30       | 30        |
      | y        | 40       | 40        |
      | width    | 320      | 320       |
      | height   | 240      | 240       |
      | flags    | 2049     | 2049      |
      | modality | 2        | 2         |
      | opacity  | 0.5      | 0.5       |
