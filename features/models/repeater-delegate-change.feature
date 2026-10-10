# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
# Measured on Qt 6.11.2 (qml6, offscreen), which does otherwise: see below.

# Qt 6.11 builds the items of a Repeater twice when its delegate changes: the
# old items go as if the model was cleared and new ones are created, then
# those go as if the model was replaced and are created again ("items" goes
# on "-1:1-0:0+0+1" and "created" is 4). Nothing is gained by the second
# round, so Ljós creates each item once.
@qt-diverges
Feature: A Repeater with a new delegate
  Changing the delegate replaces every item, once.

  Scenario: A new delegate creates each item once
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string items: ""
        property int created: 0
        function other() { rep.delegate = alt }
        Component {
          id: alt
          Item { property int row: index; Component.onCompleted: root.created++ }
        }
        Repeater {
          id: rep; objectName: "rep"; model: 2
          onItemAdded: (i, item) => root.items += "+" + i
          onItemRemoved: (i, item) => root.items += "-" + i + ":" + item.row
          Item { property int row: index }
        }
      }
      """
    When the method "other" of "root" is called
    Then the property "items" of "root" is "+0+1-0:-1-0:-1+0+1"
    And the property "created" of "root" is 2
    And the property "count" of "rep" is 2
