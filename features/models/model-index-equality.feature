# Sources:
#   https://doc.qt.io/qt-6/qmodelindex.html
#   https://doc.qt.io/qt-6/qml-qtquick-treeview.html
#   https://doc.qt.io/qt-6/qml-qt-labs-qmlmodels-treemodel.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Two model indexes of the same item are equal
  A model index is a value: two indexes of the same row and column,
  under the same parent, of the same model are equal, however each was
  made. So a property written the index it already holds has not
  changed, and what is bound to it does not run again.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.qmlmodels
      Item {
        id: root; objectName: "root"
        property string out: ""
        property var held; property int changes: 0; onHeldChanged: changes++
        ListModel { id: lm; ListElement { a: 1 } ListElement { a: 2 } }
        ListModel { id: other; ListElement { a: 1 } }
        TreeModel {
          id: tm
          TableModelColumn { display: "name" }
          rows: [ { name: "docs", rows: [ { name: "a" }, { name: "b" } ] }, { name: "bin", rows: [ { name: "c" } ] } ]
        }
        TreeView { id: tv; model: tm; property int roots: 0; onRootIndexChanged: roots++ }
        function list() {
          var a = lm.index(0, 0), b = lm.index(0, 0)
          out = [a === b, a == b, a === lm.index(1, 0), a == lm.index(1, 0), a === other.index(0, 0), lm.index(5, 0) === other.index(7, 0)].join(" ")
        }
        function tree() {
          var a = tm.index([0, 1], 0)
          out = [a === tm.index([0, 1], 0), a == tm.index([0, 1], 0), a === tm.index([1, 0], 0), a.parent === tm.index([0], 0)].join(" ")
        }
        function writeHeld() { held = lm.index(0, 0); held = lm.index(0, 0); held = lm.index(1, 0); out = changes }
        function writeRoot() { var before = tv.roots; tv.rootIndex = tm.index([0], 0); tv.rootIndex = tm.index([0], 0); out = tv.roots - before }
      }
      """

  Scenario: Two indexes of one item of a list model are equal, and others are not
    When the method "list" of "root" is called
    Then the property "out" of "root" is "true true false false false true"

  Scenario: Two indexes of one item of a tree are equal, and so are their parents
    When the method "tree" of "root" is called
    Then the property "out" of "root" is "true true false true"

  Scenario: A property written the index it holds does not change
    When the method "writeHeld" of "root" is called
    Then the property "out" of "root" is "2"

  Scenario: A view's rootIndex written the index it holds does not change
    When the method "writeRoot" of "root" is called
    Then the property "out" of "root" is "1"
