# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tableviewdelegate.html
#   https://doc.qt.io/qt-6/qtquickcontrols-basic.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: TableViewDelegate
  TableViewDelegate is the stock delegate of a TableView: the cell's
  display text, with no padding, on rows that alternate between two
  shades. The view gives it its tableView and whether it is current,
  selected or edited; a selected or current cell has the highlight.
  In the Basic style a header's sections are light with a midlight
  frame, and text is in the window text colour unless highlighted.

  Background:
    Given the file "Cells.qml":
      """qml
      import QtQuick; import QtQuick.Controls; import Qt.labs.qmlmodels
      TableView {
        id: tv; objectName: "tv"; width: 300; height: 200
        property string out: ""
        selectionModel: ItemSelectionModel {}
        model: TableModel { TableModelColumn { display: "a" } TableModelColumn { display: "b" } rows: [ { a: "one", b: 2 }, { a: 3, b: 4 } ] }
        delegate: TableViewDelegate {}
        function d(c, r) { var i = itemAtCell(c, r); return [i.tableView === tv, i.contentItem.text, i.padding, i.background.color, i.highlighted, i.current, i.selected, i.editing].join(" ") }
        function plain() { out = d(0, 0) + " | " + d(1, 1) }
        function pick() { selectionModel.setCurrentIndex(model.index(0, 0), ItemSelectionModel.Select) }
        function colours() { pick(); var h = header.itemAtCell(0, 0), b = h.background, i = itemAtCell(0, 0), j = itemAtCell(1, 1)
          out = [j.contentItem.color, j.background.color, i.contentItem.color, i.background.color, i.background.border.color,
                 h.contentItem.color, b.color, b.border.width, b.border.color].join(" ") }
        function picked() { var i = itemAtCell(0, 0); out = [i.highlighted, i.current, i.selected, i.editing, "" + i.background.color === "" + i.palette.highlight,
                                 "" + i.contentItem.color === "" + i.palette.highlightedText, i.background.border.width].join(" ") }
        HorizontalHeaderView { id: header; y: -40; syncView: tv }
      }
      """

  Scenario: A cell shows its display text on its row's shade
    Given the QML:
      """qml
      import QtQuick
      Cells {}
      """
    When 50 milliseconds pass
    And the method "plain" of "tv" is called
    Then the property "out" of "tv" is "true one 0 #ffffff false false false false | true 4 0 #f7f7f7 false false false false"

  Scenario: A current, selected cell has the highlight, and a frame of it for being current
    Given the QML:
      """qml
      import QtQuick
      Cells {}
      """
    When 50 milliseconds pass
    And the method "pick" of "tv" is called
    And 50 milliseconds pass
    And the method "picked" of "tv" is called
    Then the property "out" of "tv" is "true true true false true true 2"

  Scenario: The Basic style's cells and sections
    Given the application starts with the controls style "Basic"
    And the QML:
      """qml
      import QtQuick
      Cells {}
      """
    When 50 milliseconds pass
    And the method "colours" of "tv" is called
    Then the property "out" of "tv" is "#26282a #f7f7f7 #090909 #0066ff #0066ff #26282a #f6f6f6 1 #e4e4e4"
