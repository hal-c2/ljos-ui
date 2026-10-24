# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (reuseItems, the attached signals pooled and reused)
# Verified on Qt 6.11.2 (qml6, offscreen): a delegate in the pool reads
# visible, and so do the items in it, and is never told its visibility
# changed; it is neither drawn nor given the pointer, even where the view
# does not clip and it lies in the window.

@qt-parity @api-QtQuick.ListView.reuseItems
Feature: A delegate in a view's pool is not shown and still reads visible
  A view that reuses its delegates keeps the ones it no longer shows in a
  pool, where they wait for another row. They are not drawn and take no
  pointer, but nothing in them is told so: their `visible` stays as they
  set it, so a delegate that comes back shows its new row as it was.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: root; objectName: "root"; width: 200; height: 200; color: "white"
        property int changes: 0
        property string clicked: ""
        MouseArea { anchors.fill: parent; onClicked: parent.clicked = "under" }
        ListView {
          objectName: "list"; width: 200; height: 100; cacheBuffer: 0; reuseItems: true
          model: 200
          delegate: Rectangle {
            id: d; objectName: "r" + index; width: 200; height: 20; color: "red"
            required property int index
            property bool inPool: false
            onVisibleChanged: root.changes++
            ListView.onPooled: inPool = true
            ListView.onReused: inPool = false
            MouseArea { anchors.fill: parent; onClicked: root.clicked = "row " + d.index }
            Item { objectName: "kid" + d.index }
          }
        }
      }
      """
    When 100 milliseconds pass
    And the property "height" of "list" is set to 20
    And 100 milliseconds pass

  Scenario: The rows the view lets go wait in the pool
    Then the property "inPool" of "r1" is false
    And the property "inPool" of "r2" is true
    And the property "inPool" of "r4" is true

  Scenario: A pooled delegate and what is in it still read visible
    Then the property "visible" of "r2" is true
    And the property "visible" of "kid2" is true
    And the property "changes" of "root" is 0

  Scenario: A pooled delegate is not drawn where the view does not clip
    Then the pixel at 100, 10 of the window is "#ff0000"
    And the pixel at 100, 50 of the window is "#ffffff"
    And the pixel at 100, 90 of the window is "#ffffff"

  Scenario: A pooled delegate takes no pointer
    When the user presses the pointer at 100, 50
    And the user releases the pointer at 100, 50
    Then the property "clicked" of "root" is "under"

  Scenario: A delegate taken from the pool is drawn again and takes the pointer
    When the property "contentY" of "list" is set to 20
    And 100 milliseconds pass
    And the property "height" of "list" is set to 100
    And 100 milliseconds pass
    Then the pixel at 100, 50 of the window is "#ff0000"
    When the user presses the pointer at 100, 50
    And the user releases the pointer at 100, 50
    Then the property "clicked" of "root" is "row 3"
