# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
#   https://doc.qt.io/qt-6/qml-qtqml-instantiator.html
#   https://doc.qt.io/qt-6/qtquick-modelviewsdata-modelview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What a delegate reads of a row that was removed
  The roles of a row that left its model read undefined in its delegate. Nothing is told that
  they changed: a binding that read a role keeps what it read, until something else it reads
  changes.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string log: ""
        function swap() { rep.model = other; inst.model = other }
        ListModel { id: other; ListElement { name: "z" } }
        ListModel { id: lm; objectName: "lm"; ListElement { name: "a" } ListElement { name: "b" } }
        Repeater {
          id: rep; model: lm
          onItemAdded: (i, item) => root.log += "+" + item.seen
          onItemRemoved: (i, item) => root.log += "-" + item.now() + "/" + item.seen + "/" + item.at
          Item {
            property string seen: name
            property string at: index + ":" + name
            function now() { return name + "," + model.name + "," + modelData }
            Component.onDestruction: root.log += "~" + name
          }
        }
        Instantiator {
          id: inst; model: lm
          onObjectRemoved: (i, o) => root.log += "-" + o.now()
          QtObject { function now() { return name + "," + model.name } }
        }
      }
      """

  Scenario: The roles of a removed row read undefined
    When the method "remove" of "lm" is called with 0
    Then the property "log" of "root" is "+a+b-undefined,undefined-undefined,undefined,undefined/a/-1:undefined~undefined"

  Scenario: Every row of a cleared model reads undefined
    When the method "clear" of "lm" is called
    Then the property "log" of "root" is "+a+b-undefined,undefined-undefined,undefined-undefined,undefined,undefined/a/-1:undefined~undefined-undefined,undefined,undefined/b/-1:undefined~undefined"

  Scenario: A delegate replaced with its whole model keeps its roles
    When the method "swap" of "root" is called
    Then the property "log" of "root" is "+a+b-b,b,b/b/1:b~b-a,a,a/a/0:a~a+z-a,a-b,b"

  Scenario: A row that stays keeps its roles
    When the method "remove" of "lm" is called with 0
    And the method "setProperty" of "lm" is called with 0, "name", "c"
    And the method "remove" of "lm" is called with 0
    Then the property "log" of "root" is "+a+b-undefined,undefined-undefined,undefined,undefined/a/-1:undefined~undefined-undefined,undefined-undefined,undefined,undefined/c/-1:undefined~undefined"
