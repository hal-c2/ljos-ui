# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-viewtransition.html#targetIndexes-attached-prop
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html#addDisplaced-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.addDisplaced
Feature: The rows added before a layout are its targets, in order
  Rows a script adds one after another before the view lays them out are one change to
  it: the rows they push aside are told all of them in `ViewTransition.targetIndexes`,
  each where it is once all have arrived, from the first row down.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 200
        property string targets
        ListModel { id: m; ListElement { n: 0 } ListElement { n: 1 } ListElement { n: 2 } }
        ListView {
          anchors.fill: parent; model: m
          delegate: Rectangle { width: 100; height: 20 }
          addDisplaced: Transition {
            id: shove
            ScriptAction { script: root.targets = shove.ViewTransition.targetIndexes.join(",") }
          }
        }
        function appended() { m.insert(1, { n: 3 }); m.append({ n: 4 }); m.append({ n: 5 }) }
        function before() { m.insert(0, { n: 3 }); m.insert(0, { n: 4 }); m.append({ n: 5 }); m.insert(2, { n: 6 }) }
      }
      """

  Scenario: Rows appended after one inserted are told where they arrived
    When 50 milliseconds pass
    And the method "appended" of "root" is called
    And 100 milliseconds pass
    Then the property "targets" of "root" is "1,4,5"

  Scenario: Rows inserted before ones added earlier move those, and all are told in order
    When 50 milliseconds pass
    And the method "before" of "root" is called
    And 100 milliseconds pass
    Then the property "targets" of "root" is "0,1,2,6"
