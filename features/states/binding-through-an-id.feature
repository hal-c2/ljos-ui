# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-binding.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQml.Binding.target @api-QtQml.Binding.property
Feature: A Binding that names its property through an id
  A Binding may name what it binds as `id.property: value`, in place of
  a target and a property: the object with that id has the value, and
  follows it.

  Scenario: The property named through an id follows the value
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int source: 3
        QtObject { id: other; objectName: "other"; property int n: 0 }
        Binding { other.n: root.source * 2 }
      }
      """
    Then the property "n" of "other" is 6
    When the property "source" of "root" is set to 5
    Then the property "n" of "other" is 10
    And the log has no warning matching "non-existent"

  Scenario: The value it gave goes when its condition is false
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property bool on: true
        QtObject { id: other; objectName: "other"; property int n: 1 }
        Binding { other.n: 7; when: root.on }
      }
      """
    Then the property "n" of "other" is 7
    When the property "on" of "root" is set to false
    Then the property "n" of "other" is 1
    When the property "on" of "root" is set to true
    Then the property "n" of "other" is 7
