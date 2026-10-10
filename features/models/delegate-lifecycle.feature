# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
#   https://doc.qt.io/qt-6/qml-qtqml-instantiator.html
#   https://doc.qt.io/qt-6/qml-qtqml-component.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: When a delegate completes and when it is destroyed
  A delegate is complete before its Repeater or Instantiator says it was
  added, and Component.onDestruction runs right after the one that made it
  said it was removed: before the next delegate is removed.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        ListModel { id: lm; objectName: "lm"; ListElement { name: "a" } ListElement { name: "b" } }
        Repeater { id: rep; objectName: "rep"; model: lm
          onItemAdded: (i, item) => root.log += " +" + item.keep
          onItemRemoved: (i, item) => root.log += " -" + item.keep
          Item { id: d; property string keep: name
            Component.onCompleted: root.log += " c:" + d.keep
            Component.onDestruction: root.log += " ~" + d.keep
            Item { Component.onDestruction: root.log += " ~~" + d.keep }
          }
        }
        Instantiator { id: inst; objectName: "inst"; model: lm
          onObjectAdded: (i, o) => root.log += " +i" + o.keep
          onObjectRemoved: (i, o) => root.log += " -i" + o.keep
          QtObject { id: q; property string keep: name
            Component.onCompleted: root.log += " ci:" + q.keep
            Component.onDestruction: root.log += " ~i" + q.keep
          }
        }
        function forget() { log = "" }
        function none() { rep.model = 0; inst.model = 0 }
      }
      """

  Scenario: Each delegate completes before it is said to be added
    Then the property "log" of "root" is " ci:a +ia ci:b +ib c:a +a c:b +b"

  Scenario: A new row's delegate completes before it is said to be added
    When the method "forget" of "root" is called
    And the method "append" of "lm" is called with {"name": "c"}
    Then the property "log" of "root" is " ci:c +ic c:c +c"

  Scenario: A removed row's delegate is destroyed right after it was said to be removed
    When the method "forget" of "root" is called
    And the method "remove" of "lm" is called with 0
    Then the property "log" of "root" is " -ia ~ia -a ~~a ~a"

  Scenario: The delegates of a cleared model are destroyed one after another
    When the method "forget" of "root" is called
    And the method "clear" of "lm" is called
    Then the property "log" of "root" is " -ia ~ia -ib ~ib -a ~~a ~a -b ~~b ~b"

  Scenario: The delegates of a replaced model are destroyed one after another
    When the method "forget" of "root" is called
    And the method "none" of "root" is called
    Then the property "log" of "root" is " -b ~~b ~b -a ~~a ~a -ia ~ia -ib ~ib"
