# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html
#   https://doc.qt.io/qt-6/qml-qtquick-treeview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What a TreeView attaches to its delegates
  A delegate of a TreeView reads its view as TableView.view. With
  reuseItems, a delegate that scrolls out of sight is pooled and one
  taken from the pool for another row is reused, and each is told by
  its attached object. editDelegate is null while nothing is edited.

  Background:
    Given the file "Tree.qml":
      """qml
      import QtQuick
      import Qt.labs.qmlmodels
      TreeView {
        id: tv; objectName: "tv"; width: 200; height: 100
        property string out: ""
        property string seen: ""
        property int pooled: 0
        property int reused: 0
        property var items: { var r = []; for (var i = 0; i < 40; ++i) r.push({ name: "n" + i }); return r }
        model: TreeModel { TableModelColumn { display: "name" } rows: tv.items }
        delegate: Item {
          implicitWidth: 60; implicitHeight: 20
          TableView.onPooled: tv.pooled++
          TableView.onReused: tv.reused++
          Component.onCompleted: if (row === 0) tv.seen = (TableView.view === tv) + " " + TableView.editDelegate
        }
        function down() { contentY = 300 }
        function back() { contentY = 0 }
        function look() { out = seen }
        function go() { out = seen + " " + (pooled > 0) + " " + (reused > 0) }
      }
      """

  Scenario: A delegate's TableView.view is the tree view
    Given the QML:
      """qml
      import QtQuick
      Tree {}
      """
    When the method "look" of "tv" is called
    Then the property "out" of "tv" is "true null"

  Scenario: With reuseItems delegates out of sight are pooled and reused
    Given the QML:
      """qml
      import QtQuick
      Tree {}
      """
    When the method "down" of "tv" is called
    And 50 milliseconds pass
    And the method "back" of "tv" is called
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "true null true true"

  Scenario: Without reuseItems no delegate is pooled
    Given the QML:
      """qml
      import QtQuick
      Tree { reuseItems: false }
      """
    When the method "down" of "tv" is called
    And 50 milliseconds pass
    And the method "back" of "tv" is called
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "true null false false"

  Scenario: A reused delegate shows the row it was taken for
    Given the QML:
      """qml
      import QtQuick
      Tree {
        delegate: Item {
          required property string display
          objectName: "cell " + display
          implicitWidth: 60; implicitHeight: 20
          TableView.onReused: tv.reused++
        }
      }
      """
    When the method "down" of "tv" is called
    And 50 milliseconds pass
    Then the property "visible" of "cell n15" is true

  Scenario: An item outside a table view has no view
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"; property string out: ""
        Item { id: inner }
        Component.onCompleted: out = "" + inner.TableView.view
      }
      """
    Then the property "out" of "root" is "null"
