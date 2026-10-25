# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.6
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html
#   https://doc.qt.io/qt-6/qml-qtquick-taphandler.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Items that take the pointer and have no accessible role
  A game board or a clock face is often drawn with rectangles, images and
  mouse areas that have no accessible role or name, so its outline is a
  window with hidden nodes only. Asked to, the snapshot also lists each
  shown item that takes the pointer as a `pointer` line, named by the text
  in it, with a reference the click and touch tools take. What the
  accessibility tree reports does not change.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 300; height: 200
        property int letters: 0
        property int arrows: 0
        Rectangle {
          x: 10; y: 10; width: 40; height: 40
          Text { anchors.centerIn: parent; text: "Q" }
          MouseArea { objectName: "tile"; anchors.fill: parent; onClicked: root.letters += 1 }
        }
        Image { id: arrow; x: 100; y: 10; width: 30; height: 30; TapHandler { onTapped: root.arrows += 1 } }
        Rectangle { x: 150; y: 10; width: 30; height: 30 }
        MouseArea { objectName: "gone"; x: 200; y: 10; width: 30; height: 30; visible: false }
      }
      """
    And the inspector is connected with the capabilities "inspect, input"

  Scenario: Without being asked the outline has only what the tree exposes
    When the agent requests a snapshot
    Then the snapshot does not contain "pointer"

  Scenario: Asked to, the snapshot lists the shown items that take the pointer
    When the agent calls the tool "app_snapshot" with:
      """json
      {"pointer": true}
      """
    Then the snapshot is:
      """text
      - window [e1] 400x400
        - pointer "Q" [e2] obj=tile
        - pointer [e3] id=arrow ⚠unnamed
        - (hidden: 1 node)
      """

  Scenario: A click on the reference of a pointer line presses the item
    Given the agent has called the tool "app_snapshot" with:
      """json
      {"pointer": true}
      """
    When the agent calls the tool "app_click" on the reference of the "pointer" named "Q"
    Then the property "letters" of "root" is 1

  Scenario: A tap on an item by its id reaches its handler
    When the agent calls the tool "app_touch" with:
      """json
      {"target": "id=arrow", "action": "tap"}
      """
    Then the property "arrows" of "root" is 1

  Scenario: The lines are listed until the agent says to stop
    Given the agent has called the tool "app_snapshot" with:
      """json
      {"pointer": true}
      """
    When the agent requests a snapshot
    Then the snapshot contains "pointer \"Q\""
    When the agent calls the tool "app_snapshot" with:
      """json
      {"pointer": false}
      """
    Then the snapshot does not contain "pointer"
