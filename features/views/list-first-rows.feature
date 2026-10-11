# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (attached signal add; the populate transition
#   is for the rows a view is first made with)
# Verified on Qt 6.11.2 (qml6, offscreen).

@api-QtQuick.ListView.add
Feature: The first rows of a model that starts empty
  A view whose model has no rows yet has nothing to start with, so every row that comes later
  arrives: its delegate is told it was added.

  Background:
    Given the QML:
      """qml
      import QtQuick
      ListView {
        id: list
        objectName: "list"
        width: 200; height: 300
        property string log: ""
        model: lm
        ListModel { id: lm; objectName: "lm" }
        delegate: Item {
          required property string name
          required property int index
          objectName: name; width: 200; height: 20
          ListView.onAdd: list.log += "+" + name + index
          ListView.onRemove: list.log += "-" + name + index
        }
      }
      """
    When 100 milliseconds pass

  @qt-parity
  Scenario: The first row is announced to its delegate
    When the method "append" of "lm" is called with {"name": "ada"}
    And 100 milliseconds pass
    Then the property "log" of "list" is "+ada0"
    And the property "y" of "ada" is 0

  @qt-parity
  Scenario: The rows after it are announced too
    When the method "append" of "lm" is called with {"name": "ada"}
    And the method "append" of "lm" is called with {"name": "bo"}
    And 100 milliseconds pass
    Then the property "log" of "list" is "+ada0+bo1"
    And the property "y" of "bo" is 20

  @qt-parity
  Scenario: A row added after the model was emptied is announced
    When the method "append" of "lm" is called with {"name": "ada"}
    And 100 milliseconds pass
    And the method "clear" of "lm" is called
    And 100 milliseconds pass
    And the method "append" of "lm" is called with {"name": "cy"}
    And 100 milliseconds pass
    Then the property "log" of "list" is "+ada0-ada-1+cy0"
    And the property "count" of "list" is 1

  # Qt lays a view out when the frame is prepared, so a row that comes and goes between two
  # frames never has a delegate there and nothing is announced (qml6 logs "+cy0" only). Ljós
  # lays the view out when the model changes: `count`, `currentItem` and the delegates are
  # right as soon as the change returns, and a row that was there for a moment is announced.
  @qt-diverges
  Scenario: A row that comes and goes between two frames is announced
    When the method "append" of "lm" is called with {"name": "ada"}
    And the method "clear" of "lm" is called
    And 100 milliseconds pass
    And the method "append" of "lm" is called with {"name": "cy"}
    And 100 milliseconds pass
    Then the property "log" of "list" is "+ada0-ada-1+cy0"
