# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (remove, removeDisplaced)
#   https://doc.qt.io/qt-6/qml-qtquick-viewtransition.html (index, targetIndexes)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.removeDisplaced @api-QtQuick.ViewTransition.targetIndexes
Feature: Rows removed together are one change to a view
  Rows a model loses before the view next lays its rows out are one removal to the view's
  transitions. Each removed row is numbered as the model numbers it once the rows removed
  before it in the model are gone, a run of removed rows counting from where it starts, and
  `targetIndexes` lists them from the last to the first.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 400
        property var seen: []; property string told: seen.join(" / "); property int index: -2
        property var removing: []; property string removeTold: removing.join(" / ")
        function drop(spec) { for (const p of spec.split(" ")) { const n = p.split(","); lm.remove(Number(n[0]), Number(n[1])) } }
        ListModel { id: lm; objectName: "lm"; Component.onCompleted: { for (let i = 0; i < 6; i++) append({ n: i }) } }
        ListView {
          anchors.fill: parent; model: lm
          delegate: Rectangle { width: 100; height: 20 }
          remove: Transition { id: rm; ScriptAction { script: { root.index = rm.ViewTransition.index; const t = String(rm.ViewTransition.targetIndexes); if (root.removing.indexOf(t) < 0) root.removing = root.removing.concat([t]) } } }
          removeDisplaced: Transition { id: rd
            ScriptAction { script: { const t = String(rd.ViewTransition.targetIndexes); if (root.seen.indexOf(t) < 0) root.seen = root.seen.concat([t]) } } }
        }
      }
      """

  Scenario Outline: Rows removed one after another are told as one change
    When 50 milliseconds pass
    And the method "drop" of "root" is called with "<removals>"
    And 100 milliseconds pass
    Then the property "told" of "root" is "<told>"

    Examples:
      | removals    | told  |
      | 4,1 0,1 1,1 | 2,1,0 |
      | 0,1 0,1 0,1 | 2,1,0 |
      | 2,1 1,1 0,1 | 2,1,0 |
      | 1,1 3,1     | 3,1   |
      | 3,1 1,1     | 2,1   |
      | 1,2 3,1     | 3,2,1 |
      | 1,1 4,1     | 4,1   |
      | 5,1 0,1     | 4,0   |
      | 2,1         | 2     |

  Scenario: One removed row is told its own index
    When 50 milliseconds pass
    And the method "drop" of "root" is called with "3,1"
    And 100 milliseconds pass
    Then the property "index" of "root" is 3

  # Blocked on B30 (the scene's polish loop, ADR 0001): qml6 runs `remove` in the layout pass,
  # once for the whole change; Ljós runs it as each row goes, told that row alone.
  @blocked
  Scenario: The remove transition is told all the rows removed together
    When 50 milliseconds pass
    And the method "drop" of "root" is called with "4,1 0,1 1,1"
    And 100 milliseconds pass
    Then the property "removeTold" of "root" is "2,1,0"
