# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html#details
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity
Feature: What is declared in a view stays on the view
  A Flickable puts its children on its contentItem, where they scroll. A
  ListView or a GridView makes its rows there and leaves what else is
  declared in it on itself, so a note over the rows stays put while they
  move.

  Rule: A ListView keeps them, and its rows go under them

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 400; height: 300
          property string order: {
            var names = []
            for (var i = 0; i < list.children.length; i++) {
              var c = list.children[i]
              names.push(c === list.contentItem ? "content" : c === note ? "note" : "other")
            }
            return names.join(",")
          }
          property real noteY: 0
          function measure() { noteY = note.mapToItem(list, 0, 0).y }
          function scroll() { list.contentY = 300; measure() }
          property bool noteOnList: note.parent === list
          ListView {
            id: list; width: 400; height: 300; model: 40
            header: Item { width: 400; height: 40 }
            delegate: Rectangle { required property int index; width: 400; height: 30 }
            Text { id: note; anchors.top: parent.top; text: "note" }
          }
        }
        """

    Scenario: A child of a ListView has the ListView for its parent
      Then the property "noteOnList" of "root" is true

    Scenario: The rows stay under what was declared
      Then the property "order" of "root" is "content,note"

    Scenario: A child of a ListView stays put while the rows scroll
      When the method "measure" of "root" is called
      Then the property "noteY" of "root" is 0
      When the method "scroll" of "root" is called
      Then the property "noteY" of "root" is 0

  Rule: Other views keep to what each is

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool gridOnGrid: gridChild.parent === grid
          property bool flickOnContent: flickChild.parent === flick.contentItem
          property bool tableOnContent: tableChild.parent === table.contentItem
          GridView { id: grid; width: 40; height: 40; model: 2; delegate: Item {}
            Item { id: gridChild } }
          Flickable { id: flick; width: 40; height: 40
            Item { id: flickChild } }
          TableView { id: table; width: 40; height: 40
            Item { id: tableChild } }
        }
        """

    Scenario: A child of a GridView has the GridView for its parent
      Then the property "gridOnGrid" of "root" is true

    Scenario: A child of a Flickable is on its contentItem
      Then the property "flickOnContent" of "root" is true

    Scenario: A child of a TableView is on its contentItem
      Then the property "tableOnContent" of "root" is true
