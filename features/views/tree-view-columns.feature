# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html
#   https://doc.qt.io/qt-6/qml-qtquick-treeview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Only the columns of a TreeView in sight have delegates
  A column that is out of sight across has no delegates, as a row out
  of sight has none; leftColumn and rightColumn name the columns in
  sight. A column not measured yet counts as wide as the average of
  those that were, and a column the columnWidthProvider makes 0 wide is
  hidden.

  Background:
    Given the file "Tree.qml":
      """qml
      import QtQuick
      import Qt.labs.qmlmodels
      TreeView {
        id: tv; objectName: "tv"; width: 100; height: 100
        property string out: ""
        model: TreeModel {
          TableModelColumn { display: "a" } TableModelColumn { display: "b" }
          TableModelColumn { display: "c" } TableModelColumn { display: "d" }
          rows: [ { a: 1, b: 2, c: 3, d: 4 }, { a: 5, b: 6, c: 7, d: 8 } ]
        }
        delegate: Item { implicitWidth: 80; implicitHeight: 20 }
        function go() {
          var s = []
          for (var c = 0; c < 4; ++c) s.push(itemAtCell(Qt.point(c, 0)) ? 1 : 0)
          out = s.join("") + " " + leftColumn + "-" + rightColumn + " w" + contentWidth
        }
        function across() { contentX = 200 }
        function back() { contentX = 0 }
      }
      """

  Scenario: Columns beyond the right edge have no delegates
    Given the QML:
      """qml
      import QtQuick
      Tree {}
      """
    When the method "go" of "tv" is called
    Then the property "out" of "tv" is "1100 0-1 w320"

  Scenario: Moving the content across gives delegates to the columns that come into sight
    Given the QML:
      """qml
      import QtQuick
      Tree {}
      """
    When the method "across" of "tv" is called
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "0011 2-3 w320"

  Scenario: Moving back gives the first columns their delegates again
    Given the QML:
      """qml
      import QtQuick
      Tree {}
      """
    When the method "across" of "tv" is called
    And 50 milliseconds pass
    And the method "back" of "tv" is called
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "1100 0-1 w320"

  Scenario: A view wide enough for every column has a delegate in each
    Given the QML:
      """qml
      import QtQuick
      Tree { width: 400 }
      """
    When the method "go" of "tv" is called
    Then the property "out" of "tv" is "1111 0-3 w320"

  Scenario: A column the provider makes 0 wide is hidden
    Given the QML:
      """qml
      import QtQuick
      Tree { columnWidthProvider: function(c) { return c === 0 ? 0 : 50 } }
      """
    When the method "go" of "tv" is called
    Then the property "out" of "tv" is "0110 1-2 w150"
