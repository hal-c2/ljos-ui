# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-gridlayout.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A state gives a layout's children their attached values
  A `PropertyChanges` may set what is attached to its target, as
  `Layout.preferredWidth: 100` sets the target's `Layout` attached
  property: the layout follows it while the state holds and the value
  before comes back when the state is left. With a grid's `flow` and
  `columns` from the same state, the children go to the cells the state
  names.

  Scenario: An attached value is set while the state holds
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "row"
        width: 800; height: 400
        property string dump: [a.x, a.width, b.x, b.width].join(" ")
        states: State {
          name: "wide"
          PropertyChanges { target: a; Layout.preferredWidth: 100 }
        }
        RowLayout {
          Rectangle { id: a; implicitWidth: 10; implicitHeight: 10 }
          Rectangle { id: b; implicitWidth: 10; implicitHeight: 10 }
        }
      }
      """
    When 100 milliseconds pass
    Then the property "dump" of "row" is "0 10 15 10"
    When the property "state" of "row" is set to "wide"
    And 100 milliseconds pass
    Then the property "dump" of "row" is "0 100 105 10"
    When the property "state" of "row" is set to ""
    And 100 milliseconds pass
    Then the property "dump" of "row" is "0 10 15 10"

  Scenario: A grid's flow, columns and cells come from one state
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "grid"
        width: 800; height: 400
        property string dump: [a.x, a.y, a.width, a.height, b.x, b.y, b.width,
          c.x, c.y, c.width, c.height, grid.x, grid.width].join(" ")
        state: "landscape"
        states: State {
          name: "landscape"
          PropertyChanges {
            target: grid; flow: GridLayout.LeftToRight; rows: 2; columns: 3
          }
          PropertyChanges {
            target: a; Layout.column: 0
            Layout.preferredWidth: root.width / 4
            Layout.preferredHeight: root.height / 5
          }
          PropertyChanges {
            target: b; Layout.column: 2
            Layout.preferredWidth: root.width / 2
            Layout.preferredHeight: root.height / 1.6
          }
          PropertyChanges {
            target: c; Layout.row: 1; Layout.columnSpan: 3
            Layout.preferredWidth: root.width / 4
            Layout.preferredHeight: root.height / 8
          }
        }
        GridLayout {
          id: grid
          flow: GridLayout.TopToBottom
          anchors.horizontalCenter: parent.horizontalCenter
          Rectangle { id: a }
          Rectangle { id: b }
          Rectangle { id: c; Layout.minimumWidth: 150; Layout.minimumHeight: 40 }
        }
      }
      """
    When 100 milliseconds pass
    Then the property "dump" of "grid" is "0 85 200 80 210 0 400 0 255 200 50 95 610"
