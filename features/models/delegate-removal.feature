# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
#   https://doc.qt.io/qt-6/qml-qtqml-instantiator.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What a delegate reads when it is removed
  A delegate whose row left the model reads index -1; one replaced with its whole model keeps
  its index.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string items: ""
        property string objects: ""
        function swap() { rep.model = 2; inst.model = 2 }
        function other() { rep.delegate = alt; inst.delegate = plain }
        Component { id: alt; Item { property int row: index } }
        Component { id: plain; QtObject { property int row: index } }
        ListModel { id: lm; objectName: "lm"; ListElement { n: "a" } ListElement { n: "b" } }
        Repeater {
          id: rep; objectName: "rep"; model: lm
          onItemAdded: (i, item) => root.items += "+" + i
          onItemRemoved: (i, item) => root.items += "-" + i + ":" + item.row
          Item { property int row: index }
        }
        Instantiator {
          id: inst; objectName: "inst"; model: lm
          onObjectAdded: (i, o) => root.objects += "+" + i
          onObjectRemoved: (i, o) => root.objects += "-" + i + ":" + o.row
          QtObject { property int row: index }
        }
      }
      """

  Scenario: A delegate whose row was removed reads index -1
    When the method "remove" of "lm" is called with 0
    Then the property "items" of "root" is "+0+1-0:-1"
    And the property "objects" of "root" is "+0+1-0:-1"

  Scenario: Clearing the model removes each delegate from the front
    When the method "clear" of "lm" is called
    Then the property "items" of "root" is "+0+1-0:-1-0:-1"
    And the property "objects" of "root" is "+0+1-0:-1-0:-1"

  Scenario: A delegate replaced with its model keeps its index
    When the method "swap" of "root" is called
    Then the property "items" of "root" is "+0+1-1:1-0:0+0+1"
    And the property "objects" of "root" is "+0+1-0:0-1:1+0+1"

  Scenario: A new delegate takes the old objects as clearing the model would
    When the method "other" of "root" is called
    Then the property "objects" of "root" is "+0+1-0:-1-0:-1+0+1"
    And the property "count" of "inst" is 2
    And the property "count" of "rep" is 2
