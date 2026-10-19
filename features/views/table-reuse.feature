# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html (Reusing items, reuseItems, the attached signals pooled and reused)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.TableView.reuseItems
Feature: A TableView reuses its delegates
  A TableView reuses its delegates unless told not to: a row that scrolls out goes to the pool
  and its delegate shows the next row that scrolls in. It is not completed again, what reads the
  row runs again after it is told it was reused, and what does not read the row stays. The counts
  are kept in a plain object, which no binding follows.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 100
        property var n: ({ created: 0, pooled: 0, reused: 0, destroyed: 0, row: 0, other: 0, stale: 0 })
        property string out: ""
        function bump(k) { n[k] = n[k] + 1 }
        function go(keys) { out = keys.split(" ").map(k => k + " " + n[k]).join(" ") }
        TableView {
          objectName: "table"
          anchors.fill: parent
          model: 200
          delegate: Item {
            id: d
            objectName: "r" + row
            implicitWidth: 200; implicitHeight: 20
            property int twice: { root.bump("row"); return row * 2 }
            property real other: { root.bump("other"); return root.width }
            Component.onCompleted: root.bump("created")
            Component.onDestruction: root.bump("destroyed")
            TableView.onPooled: root.bump("pooled")
            TableView.onReused: { root.bump("reused"); if (d.twice !== 2 * row) root.bump("stale") }
          }
        }
      }
      """
    When 100 milliseconds pass

  Scenario: Reuse is on unless turned off
    Then the property "reuseItems" of "table" is true

  Scenario: Scrolling row by row makes no new delegates
    When the property "contentY" of "table" is set to 20
    And 50 milliseconds pass
    And the property "contentY" of "table" is set to 40
    And 50 milliseconds pass
    And the property "contentY" of "table" is set to 60
    And 50 milliseconds pass
    And the method "go" of "root" is called with "created reused destroyed"
    Then the property "out" of "root" is "created 6 reused 3 destroyed 0"

  Scenario: What reads the row runs again after the delegate is told
    When the property "contentY" of "table" is set to 20
    And 50 milliseconds pass
    And the property "contentY" of "table" is set to 40
    And 50 milliseconds pass
    And the method "go" of "root" is called with "row other stale"
    Then the property "out" of "root" is "row 8 other 6 stale 0"
    And the property "twice" of "r6" is 12

  Scenario: Turning reuse off makes a delegate for each row that scrolls in
    When the property "reuseItems" of "table" is set to false
    And the property "contentY" of "table" is set to 20
    And 50 milliseconds pass
    And the property "contentY" of "table" is set to 40
    And 50 milliseconds pass
    And the method "go" of "root" is called with "created reused"
    Then the property "out" of "root" is "created 8 reused 0"
