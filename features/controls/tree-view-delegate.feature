# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-treeviewdelegate.html
#   https://doc.qt.io/qt-6/qml-qtquick-treeview.html
#   https://doc.qt.io/qt-6/qtquickcontrols-basic.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: TreeViewDelegate
  TreeViewDelegate is the stock delegate of a TreeView: the row's display
  text, moved right by its depth, after an indicator that shows on a row
  with rows under it and expands or collapses the row when tapped.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      import Qt.labs.qmlmodels
      TreeView {
        id: tv; objectName: "tv"; width: 300; height: 300
        property string out: ""
        selectionModel: ItemSelectionModel {}
        model: TreeModel {
          TableModelColumn { display: "name" }
          rows: [ { name: "docs", rows: [ { name: "sub", rows: [ { name: "deep" } ] } ] }, { name: "bin" } ]
        }
        delegate: TreeViewDelegate {}
        function d(r) { return itemAtCell(Qt.point(0, r)) }
        function metrics() { var c = d(0); out = [c.indentation, c.leftMargin, c.rightMargin, c.implicitHeight, c.text === "", c.contentItem.text, c.leftPadding].join(" ") }
        function pads() { out = [d(0).leftPadding, d(1).leftPadding, d(2).leftPadding, d(2).indicator.x].join(" ") }
        function marks() { var s = []; for (var r = 0; r < rows; ++r) { var c = d(r); s.push(c.contentItem.text + (c.expanded ? "+" : "") + (c.current ? "*" : "") + (c.selected ? "#" : "") + (c.indicator.visible ? ">" : "")) } out = s.join(" ") + " " + selectionModel.hasSelection }
        function shades() { out = [d(0).background.color, d(1).background.color].join(" ") }
      }
      """

  Scenario: The delegate shows the row's display text after room for an indicator
    When 50 milliseconds pass
    And the method "metrics" of "tv" is called
    Then the property "out" of "tv" is "20 5 5 24 true docs 30"

  Scenario: Each level of depth moves a row right by the indentation
    When 50 milliseconds pass
    And the method "expandRecursively" of "tv" is called
    And 50 milliseconds pass
    And the method "pads" of "tv" is called
    Then the property "out" of "tv" is "30 50 70 45"

  Scenario: Only a row with rows under it shows the indicator
    When 50 milliseconds pass
    And the method "marks" of "tv" is called
    Then the property "out" of "tv" is "docs> bin false"

  Scenario: Tapping a row makes it the current one without selecting it
    When 50 milliseconds pass
    And the user presses the pointer at 40, 36
    And the user releases the pointer at 40, 36
    And 50 milliseconds pass
    And the method "marks" of "tv" is called
    Then the property "out" of "tv" is "docs> bin* false"

  Scenario: Tapping the indicator expands the row
    When 50 milliseconds pass
    And the user presses the pointer at 15, 12
    And the user releases the pointer at 15, 12
    And 50 milliseconds pass
    And the method "marks" of "tv" is called
    Then the property "out" of "tv" is "docs+> sub> bin false"

  Scenario: Rows alternate between the base and the alternate base colour
    When 50 milliseconds pass
    And the method "shades" of "tv" is called
    Then the property "out" of "tv" is "#ffffff #f7f7f7"
