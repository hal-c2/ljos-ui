# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (highlightItem, currentIndex, count)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.highlightItem
Feature: The highlight item without a highlight component
  A view has a highlight item for its current row even when no `highlight` is given.

  Background:
    Given the QML:
      """qml
      import QtQuick
      ListView {
        objectName: "list"
        width: 200; height: 300
        model: lm
        ListModel { id: lm; objectName: "lm" }
        delegate: Item {
          required property string name
          objectName: name; width: 180; height: 20
        }
      }
      """

  Scenario: An empty view has no current row and no highlight item
    Then the property "currentIndex" of "list" is -1
    And the property "currentItem" of "list" is null
    And the property "highlightItem" of "list" is null

  Scenario: The first row added becomes current
    When the method "append" of "lm" is called with {"name": "ada"}
    And 100 milliseconds pass
    Then the property "currentIndex" of "list" is 0
    And the property "currentItem.objectName" of "list" is "ada"
    And the property "highlightItem.width" of "list" is 180
    And the property "highlightItem.height" of "list" is 20

  Scenario: Clearing the model leaves no current row
    When the method "append" of "lm" is called with {"name": "ada"}
    And 100 milliseconds pass
    And the method "clear" of "lm" is called
    And 100 milliseconds pass
    Then the property "count" of "list" is 0
    And the property "currentIndex" of "list" is -1
    And the property "currentItem" of "list" is null
    And the property "contentHeight" of "list" is 0
