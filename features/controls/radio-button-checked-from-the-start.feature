# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html#autoExclusive-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-radiobutton.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Radio buttons written checked
  Radio buttons are exclusive once they are made: a file that writes
  `checked: true` on more than one in a parent shows each of them checked.
  The next one checked unchecks the first of the others that is, whether
  or not that one is enabled.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Column {
        objectName: "root"
        RadioButton { id: a; objectName: "a"; text: "A"; enabled: false; checked: true }
        RadioButton { id: b; objectName: "b"; text: "B"; checked: true }
        RadioButton { id: c; objectName: "c"; text: "C" }
        RadioButton { id: d; objectName: "d"; text: "D"; checked: true }
        property string states: [a, b, c, d].map(r => r.checked ? "x" : "-").join("")
      }
      """

  Scenario: Each one written checked is checked
    Then the property "states" of "root" is "xx-x"

  Scenario: Checking another unchecks the first of the others, disabled or not
    When the property "checked" of "c" is set to true
    Then the property "states" of "root" is "-xxx"

  Scenario: Then the next of them
    When the property "checked" of "c" is set to true
    And the property "checked" of "a" is set to true
    Then the property "states" of "root" is "x-xx"
