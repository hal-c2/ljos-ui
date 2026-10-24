# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (displaced)
#   https://doc.qt.io/qt-6/qml-qtquick-viewtransition.html
# Verified on Qt 6.11.2 (qml6, offscreen): a displaced transition given to
# a view that had none runs for the next change, even one in the same
# handler.

@qt-parity @api-QtQuick.ListView.displaced
Feature: A displaced transition given to a view later
  A view without a displaced transition puts the rows a change pushes
  aside straight at their new places. Given one later, it runs it for the
  rows the next change pushes aside, from where they were.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 300
        function giveAndRemove() { list.displaced = t; lm.remove(0) }
        function give() { list.displaced = t }
        ListModel { id: lm; objectName: "lm"; ListElement { name: "a" } ListElement { name: "b" } ListElement { name: "c" } }
        Transition { id: t; NumberAnimation { properties: "x,y"; duration: 400 } }
        ListView {
          id: list; objectName: "list"; width: 200; height: 300; model: lm
          delegate: Item { required property string name; objectName: name; width: 200; height: 20 }
        }
      }
      """
    When 100 milliseconds pass

  Scenario: Without a displaced transition the rows pushed aside go straight to their places
    When the method "remove" of "lm" is called with 0
    And 150 milliseconds pass
    Then the property "y" of "b" is 0

  Scenario: A displaced transition given before the change runs for it
    When the method "give" of "root" is called
    And 100 milliseconds pass
    And the method "remove" of "lm" is called with 0
    And 150 milliseconds pass
    Then the property "y" of "b" is at least 4
    And the property "y" of "b" is at most 16
    When 500 milliseconds pass
    Then the property "y" of "b" is 0

  Scenario: A displaced transition given in the same handler as the change runs for it
    When the method "giveAndRemove" of "root" is called
    And 150 milliseconds pass
    Then the property "y" of "b" is at least 4
    And the property "y" of "b" is at most 16
    When 500 milliseconds pass
    Then the property "y" of "b" is 0
