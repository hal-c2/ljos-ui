# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
#   https://doc.qt.io/qt-6/qtquick-modelviewsdata-modelview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What a delegate's required properties read of a row that was removed
  A required property named like a role holds the role's value. When the row leaves its model
  the property keeps that value; only the index follows, to -1.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string log: ""
        ListModel { id: lm; objectName: "lm"; ListElement { name: "a" } ListElement { name: "b" } }
        Repeater {
          model: lm
          onItemAdded: (i, item) => root.log += "+" + item.name
          onItemRemoved: (i, item) => root.log += "-" + item.name + item.index + item.model.name
          Item {
            required property string name
            required property int index
            required property var model
          }
        }
      }
      """

  Scenario: A required property keeps the value of its role
    When the method "remove" of "lm" is called with 1
    Then the property "log" of "root" is "+a+b-b-1undefined"

  Scenario: A changed role is the value that is kept
    When the method "setProperty" of "lm" is called with 0, "name", "c"
    And the method "clear" of "lm" is called
    Then the property "log" of "root" is "+a+b-c-1undefined-b-1undefined"
