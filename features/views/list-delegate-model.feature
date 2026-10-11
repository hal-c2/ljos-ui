# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-models-delegatemodel.html
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQml.Models.DelegateModel.model @api-QtQml.Models.DelegateModel.delegate
Feature: A view shows the rows of a DelegateModel
  A DelegateModel is a model together with the delegate for its rows. A view given one makes
  that delegate for the rows it shows.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Item {
        id: root; objectName: "root"
        width: 200; height: 300
        function useNames() { dm.model = names }
        function useTall() { dm.delegate = tall }
        ListModel {
          id: names; objectName: "names"
          ListElement { name: "ada" }
          ListElement { name: "bo" }
        }
        Component { id: tall; Rectangle { objectName: "tall" + index; width: 200; height: 30 } }
        DelegateModel {
          id: dm; objectName: "dm"; model: 3
          delegate: Rectangle {
            objectName: "row" + index; width: 200; height: 20
            property bool mine: ListView.view === list
            property bool inside: parent === list.contentItem
            property var called: model.name
            property var number: model.modelData
          }
        }
        ListView { id: list; objectName: "list"; anchors.fill: parent; model: dm }
      }
      """
    When 100 milliseconds pass

  Scenario: The view makes the model's delegate for each row
    Then the property "count" of "list" is 3
    And the property "count" of "dm" is 3
    And the property "contentHeight" of "list" is 60
    And the property "y" of "row0" is 0
    And the property "y" of "row2" is 40
    And the property "number" of "row2" is 2

  Scenario: A delegate of the model is in the view's content and knows its view
    Then the property "inside" of "row1" is true
    And the property "mine" of "row1" is true
    And the property "currentIndex" of "list" is 0

  Scenario: The rows follow another model
    When the method "useNames" of "root" is called
    And 100 milliseconds pass
    Then the property "count" of "dm" is 2
    And the property "count" of "list" is 2
    And the property "contentHeight" of "list" is 40
    And the property "called" of "row1" is "bo"
    And the object "row2" does not exist

  Scenario: A row that is added to the model's model gets a delegate
    When the method "useNames" of "root" is called
    And the method "append" of "names" is called with {"name": "cy"}
    And 100 milliseconds pass
    Then the property "count" of "list" is 3
    And the property "called" of "row2" is "cy"
    And the property "y" of "row2" is 40

  Scenario: The rows are made again of another delegate
    When the method "useTall" of "root" is called
    And 100 milliseconds pass
    Then the property "count" of "list" is 3
    And the property "contentHeight" of "list" is 90
    And the property "y" of "tall1" is 30
    And the object "row1" does not exist

  Scenario: A model with no rows leaves the view empty
    When the property "model" of "dm" is set to 0
    And 100 milliseconds pass
    Then the property "count" of "dm" is 0
    And the property "count" of "list" is 0
    And the property "contentHeight" of "list" is 0
