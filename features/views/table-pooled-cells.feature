# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html (reuseItems, the attached signals pooled and reused)
# Verified on Qt 6.11.2 (qml6, offscreen): a cell in the pool reads visible
# and is never told its visibility changed; it is neither drawn nor given
# the pointer, even where the view does not clip and it lies in the window.

@qt-parity @api-QtQuick.TableView.reuseItems
Feature: A cell in a TableView's pool is not shown and still reads visible
  As a ListView's: a cell waiting in the pool for another row is not
  drawn and takes no pointer, and nothing in it is told so.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: top; objectName: "root"; width: 200; height: 200; color: "white"
        property int changes: 0
        property string clicked: ""
        MouseArea { anchors.fill: parent; onClicked: top.clicked = "under" }
        TableView {
          objectName: "table"; width: 200; height: 100; reuseItems: true
          model: 200
          delegate: Rectangle {
            id: d; objectName: "c" + index; implicitWidth: 200; implicitHeight: 20; color: "red"
            required property int index
            property bool inPool: false
            onVisibleChanged: top.changes++
            TableView.onPooled: inPool = true
            TableView.onReused: inPool = false
            MouseArea { anchors.fill: parent; onClicked: top.clicked = "cell " + d.index }
          }
        }
      }
      """
    When 100 milliseconds pass
    And the property "height" of "table" is set to 20
    And 100 milliseconds pass

  Scenario: A pooled cell still reads visible
    Then the property "inPool" of "c2" is true
    And the property "visible" of "c2" is true
    And the property "changes" of "root" is 0

  Scenario: A pooled cell is not drawn and takes no pointer
    Then the pixel at 100, 50 of the window is "#ffffff"
    When the user presses the pointer at 100, 50
    And the user releases the pointer at 100, 50
    Then the property "clicked" of "root" is "under"
