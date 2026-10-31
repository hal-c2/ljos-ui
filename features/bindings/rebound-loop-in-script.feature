# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#binding-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A loop through a property read in a script's scope
  The same loop where the property that closes it is read by a binding
  made in a handler, not by the property it is bound to: Qt goes round
  it once and leaves each property as it was when it changed.

  Scenario: A loop through a property read in a script's scope stops after one round
    Given the QML:
      """qml
      import QtQuick
      QtObject {
        id: root
        objectName: "scope"
        property int count: 0
        readonly property int twice: count * 2
        property int base: 3
        Component.onCompleted: count = Qt.binding(() => root.base + (root.twice > 100 ? root.twice : 1))
      }
      """
    Then the property "count" of "scope" is 4
    And the property "twice" of "scope" is 8
    When the property "base" of "scope" is set to 7
    Then the property "count" of "scope" is 8
    And the property "twice" of "scope" is 16
    When the property "base" of "scope" is set to 60
    Then the property "count" of "scope" is 61
    And the property "twice" of "scope" is 122
    When the property "base" of "scope" is set to 1
    Then the property "count" of "scope" is 123
    And the property "twice" of "scope" is 246
