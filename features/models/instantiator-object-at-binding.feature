# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-models-instantiator.html#objectAt-method
#   https://doc.qt.io/qt-6/qml-qtqml-models-instantiator.html#count-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A binding that reads an Instantiator's objects through objectAt
  A binding that walks an Instantiator's objects with objectAt, up to count, runs again when the
  Instantiator makes or drops its objects, and when a property it read through one of them changes.
  Here each object reads the binding as it is made, so the binding first runs while count already
  counts the objects but objectAt has none of them yet.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Item {
        id: root
        objectName: "root"
        property int grow: 0
        readonly property real largest: {
          let h = 0
          for (let c = 0; c < made.count; ++c) {
            const it = made.objectAt(c)
            if (it) h = Math.max(h, it.size)
          }
          return h
        }
        Instantiator {
          id: made
          objectName: "made"
          model: 3
          QtObject {
            property real seen: root.largest
            property real size: 10 + index * 5 + (index === 1 ? root.grow : 0)
          }
        }
      }
      """

  Scenario: The binding sees the objects the Instantiator made
    Then the property "largest" of "root" is 20

  Scenario: A property read through objectAt changes
    When the property "grow" of "root" is set to 30
    Then the property "largest" of "root" is 45

  Scenario: The Instantiator drops some of its objects
    When the property "model" of "made" is set to 1
    Then the property "largest" of "root" is 10

  Scenario: The Instantiator makes more objects
    When the property "model" of "made" is set to 5
    Then the property "largest" of "root" is 30
