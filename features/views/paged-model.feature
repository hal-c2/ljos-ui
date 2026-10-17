# Sources:
#   https://doc.qt.io/qt-6/qabstractitemmodel.html#canFetchMore
#   https://doc.qt.io/qt-6/qabstractitemmodel.html#fetchMore
#   https://doc.qt.io/qt-6/model-view-programming.html#lazy-population-of-model-data
# The model is one the application writes in Rust (ItemModel with
# can_fetch_more and fetch_more), as a remote listing or a long log is.
# No model QML can make answers canFetchMore, and no step can give the Qt
# backend a C++ model, so the scenarios run on Ljós only.

@ljos-only
Feature: Views of a list model that reads its rows a page at a time
  A list model may have more rows than it has read. A ListView, GridView
  or TableView that shows its last row asks the model for more, after it
  has laid its rows out, and the rows come when the model has read them.
  A view that does not reach its end does not ask.

  Background:
    Given a list model "pages" of 50 rows that reads 20 at a time, 100 ms after a view asks for more

  Scenario: A ListView reads the first page, and the next when it is scrolled to its end
    Given the QML:
      """qml
      import QtQuick
      ListView {
        id: lv; objectName: "lv"; width: 100; height: 100; cacheBuffer: 0
        model: pages
        delegate: Item { width: 100; height: 20; required property string display }
      }
      """
    When 50 milliseconds pass
    Then the property "count" of "lv" is 0
    When 100 milliseconds pass
    Then the property "count" of "lv" is 20
    When 200 milliseconds pass
    Then the property "count" of "lv" is 20
    When the method "positionViewAtEnd" of "lv" is called
    And 150 milliseconds pass
    Then the property "count" of "lv" is 40

  Scenario: A ListView that shows every row reads until the model has no more
    Given the QML:
      """qml
      import QtQuick
      ListView {
        id: lv; objectName: "lv"; width: 100; height: 2000
        model: pages
        delegate: Item { width: 100; height: 20; required property string display }
        property string last: ""
        function readLast() { last = itemAtIndex(count - 1).display }
      }
      """
    When 1 second passes
    And the method "readLast" of "lv" is called
    Then the property "count" of "lv" is 50
    And the property "last" of "lv" is "row 49"

  Scenario: A GridView reads the next page when it is scrolled to its end
    Given the QML:
      """qml
      import QtQuick
      GridView {
        id: gv; objectName: "gv"; width: 100; height: 100; cacheBuffer: 0
        cellWidth: 50; cellHeight: 20
        model: pages
        delegate: Item { width: 50; height: 20; required property string display }
      }
      """
    When 150 milliseconds pass
    Then the property "count" of "gv" is 20
    When 200 milliseconds pass
    Then the property "count" of "gv" is 20
    When the method "positionViewAtEnd" of "gv" is called
    And 150 milliseconds pass
    Then the property "count" of "gv" is 40

  Scenario: A TableView reads the next page when it is scrolled to its end
    Given the QML:
      """qml
      import QtQuick
      TableView {
        id: tv; objectName: "tv"; width: 100; height: 100
        model: pages
        delegate: Item { implicitWidth: 100; implicitHeight: 20; required property string display }
      }
      """
    When 150 milliseconds pass
    Then the property "rows" of "tv" is 20
    When 200 milliseconds pass
    Then the property "rows" of "tv" is 20
    When the property "contentY" of "tv" is set to 300
    And 150 milliseconds pass
    Then the property "rows" of "tv" is 40
