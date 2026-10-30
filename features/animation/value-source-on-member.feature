# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-modifier-objects
#   https://doc.qt.io/qt-6/qml-qtquick-numberanimation.html
# Verified on Qt 6.11.2 (qml6, offscreen), but for the scenario tagged @ljos-only: qml6's
# clock under the runner does not stop at exactly 500 ms, so the halfway value is not exact there.

@qt-parity
Feature: An animation declared on a member of a value
  "NumberAnimation on font.pixelSize { }" animates one member of a value-type property; the
  value's other members stay as they are.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        Text {
          objectName: "label"; text: "Hi"; font.bold: true; font.pixelSize: 10
          NumberAnimation on font.pixelSize { from: 10; to: 30; duration: 1000 }
        }
      }
      """

  Scenario: The member ends at to and the others stay
    When 2 seconds pass
    Then the property "font.pixelSize" of "label" is 30
    And the property "font.bold" of "label" is true

  @ljos-only
  Scenario: The member passes through the values between
    When 500 milliseconds pass
    Then the property "font.pixelSize" of "label" is 20
