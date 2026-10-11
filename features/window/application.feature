# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-application.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The application
  `Application` and `Qt.application` tell whether the application has the
  keyboard and which screens there are.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int state: Application.state
        property bool active: Application.active
        property int screens: Application.screens.length
        property int firstScreenWidth: Application.screens[0].width
        property int qtState: Qt.application.state
        property bool qtActive: Qt.application.active
        property int qtScreens: Qt.application.screens.length
        property int activeState: Qt.ApplicationActive
      }
      """

  @api-QtQuick.Application.state @api-QtQuick.Application.active @api-QtQuick.Application.screens
  Scenario Outline: A running application with a window is active on one screen
    Then the property "<property>" of "root" is <value>

    Examples:
      | property    | value |
      | activeState | 4     |
      | state       | 4     |
      | active      | true  |
      | screens     | 1     |

  @api-QtQml.Qt.application
  Scenario Outline: Qt.application tells the same
    Then the property "<property>" of "root" is <value>

    Examples:
      | property  | value |
      | qtState   | 4     |
      | qtActive  | true  |
      | qtScreens | 1     |

  @api-QtQuick.Application.screens
  Scenario: Each screen tells its size
    Then the property "firstScreenWidth" of "root" is at least 1
