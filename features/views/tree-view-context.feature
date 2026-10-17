# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-treeview.html
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What a TreeView gives a delegate and what it starts with

  Scenario: A delegate without required properties sees its cell and the roles
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.qmlmodels
      TreeView {
        id: tv; objectName: "tv"; width: 200; height: 300
        property string out: ""
        model: TreeModel {
          TableModelColumn { display: "name" }
          rows: [ { name: "docs", rows: [ { name: "a" } ] }, { name: "bin" } ]
        }
        delegate: Item {
          implicitWidth: 60; implicitHeight: 20
          Component.onCompleted: if (row === 1) tv.out = [row, column, display, model.display, modelData, typeof hasChildren, typeof isTreeNode, typeof model.depth].join(" ")
        }
      }
      """
    When 50 milliseconds pass
    Then the property "out" of "tv" is "1 0 bin bin bin undefined undefined undefined"

  Scenario: A delegate with required properties still sees the roles of its cell
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.qmlmodels
      TreeView {
        id: tv; objectName: "tv"; width: 200; height: 300
        property string out: ""
        model: TreeModel {
          TableModelColumn { display: "name" }
          rows: [ { name: "docs" }, { name: "bin" } ]
        }
        delegate: Item {
          implicitWidth: 60; implicitHeight: 20
          required property int depth
          Component.onCompleted: if (row === 1) tv.out = [row, index, display, model.display, depth].join(" ")
        }
      }
      """
    When 50 milliseconds pass
    Then the property "out" of "tv" is "1 1 bin bin 0"

  Scenario: A TreeView starts with nothing expanded, selected or current
    Given the QML:
      """qml
      import QtQuick
      TreeView {
        id: tv; objectName: "tv"; width: 200; height: 300
        property string out: [rows, columns, rootIndex.valid, selectionModel === null, reuseItems, currentRow, alternatingRows].join(" ")
        property string nav: [editTriggers, selectionBehavior, selectionMode, keyNavigationEnabled, pointerNavigationEnabled].join(" ")
      }
      """
    Then the property "out" of "tv" is "0 0 false true true -1 true"
    And the property "nav" of "tv" is "8 2 2 true true"
