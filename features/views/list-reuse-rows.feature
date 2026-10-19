# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (reuseItems, the attached signals pooled and reused)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.reuseItems
Feature: What a reused ListView delegate runs again
  A delegate taken from the pool shows another row: what reads the row (its index and roles) runs
  again, and nothing else does. It is not completed again, it keeps the state it had, and by the
  time it is told it was reused it reads its new row. The counts are kept in a plain object, which
  no binding follows, so counting runs nothing again.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 100
        property var n: ({ created: 0, pooled: 0, reused: 0, destroyed: 0, index: 0, other: 0, kept: 0, wrong: 0, shown: 0 })
        property string out: ""
        function bump(k) { n[k] = n[k] + 1 }
        function go(keys) { out = keys.split(" ").map(k => k + " " + n[k]).join(" ") }
        ListView {
          objectName: "list"; anchors.fill: parent; cacheBuffer: 0; reuseItems: true
          model: ListModel { Component.onCompleted: { for (let i = 0; i < 200; i++) append({ name: "n" + i }) } }
          delegate: Item {
            id: d; width: 200; height: 20
            required property int index; required property string name
            property int twice: { root.bump("index"); return index * 2 }
            property real other: { root.bump("other"); return root.width }
            property int times: 0
            Component.onCompleted: root.bump("created"); Component.onDestruction: root.bump("destroyed")
            ListView.onPooled: { root.bump("pooled"); if (d.visible) root.bump("shown") }
            ListView.onReused: {
              root.bump("reused"); d.times++; root.n.kept = Math.max(root.n.kept, d.times)
              if (d.name !== "n" + d.index || d.twice !== 2 * d.index) root.bump("wrong")
            }
          }
        }
      }
      """
    When 100 milliseconds pass
    And the property "contentY" of "list" is set to 200
    And 100 milliseconds pass
    And the property "contentY" of "list" is set to 400
    And 100 milliseconds pass
    And the property "contentY" of "list" is set to 600
    And 100 milliseconds pass

  Scenario: Scrolling reuses what it pooled and completes nothing again
    When the method "go" of "root" is called with "created pooled reused destroyed"
    Then the property "out" of "root" is "created 7 pooled 17 reused 17 destroyed 0"

  Scenario: A binding that reads the row runs again on reuse, and one that does not stays
    When the method "go" of "root" is called with "index other"
    Then the property "out" of "root" is "index 24 other 7"

  Scenario: A reused delegate reads its new row by the time it is told
    When the method "go" of "root" is called with "wrong"
    Then the property "out" of "root" is "wrong 0"

  Scenario: A delegate keeps its own state from row to row
    When the method "go" of "root" is called with "kept"
    Then the property "out" of "root" is "kept 3"

  Scenario: A delegate is still visible when it is told it was pooled
    When the method "go" of "root" is called with "shown"
    Then the property "out" of "root" is "shown 17"

  Scenario: The pool keeps what it holds when the view shrinks
    When the property "height" of "root" is set to 20
    And 100 milliseconds pass
    And the property "contentY" of "list" is set to 3000
    And 1000 milliseconds pass
    And the method "go" of "root" is called with "pooled destroyed"
    Then the property "out" of "root" is "pooled 23 destroyed 0"
