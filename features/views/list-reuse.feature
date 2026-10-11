# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (reuseItems, the attached signals pooled and reused)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.reuseItems
Feature: A ListView that reuses its delegates
  With `reuseItems` a delegate that scrolls out of the view is kept and shows another row later,
  so scrolling makes no new objects.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 200; height: 100
        property int created: 0
        property int pooled: 0
        property int reused: 0
        ListView {
          objectName: "list"
          anchors.fill: parent
          cacheBuffer: 0
          model: 200
          delegate: Item {
            required property int index
            objectName: "r" + index; width: 200; height: 20
            Component.onCompleted: root.created++
            ListView.onPooled: root.pooled++
            ListView.onReused: root.reused++
          }
        }
      }
      """
    When 100 milliseconds pass

  Scenario: Reuse is off unless asked for
    Then the property "reuseItems" of "list" is false

  Scenario: Without reuse scrolling makes new delegates
    When the property "contentY" of "list" is set to 1000
    And 100 milliseconds pass
    Then the property "created" of "root" is at least 10
    And the property "pooled" of "root" is 0
    And the property "reused" of "root" is 0

  Scenario: With reuse scrolling makes no new delegates
    When the property "reuseItems" of "list" is set to true
    And the property "contentY" of "list" is set to 200
    And 100 milliseconds pass
    And the property "contentY" of "list" is set to 400
    And 100 milliseconds pass
    And the property "contentY" of "list" is set to 600
    And 100 milliseconds pass
    Then the property "created" of "root" is at most 12
    And the property "pooled" of "root" is at least 10
    And the property "reused" of "root" is at least 10

  Scenario: A reused delegate shows its new row
    When the property "reuseItems" of "list" is set to true
    And the property "contentY" of "list" is set to 40
    And 100 milliseconds pass
    And the property "contentY" of "list" is set to 80
    And 100 milliseconds pass
    Then the property "y" of "r7" is 140
    And the property "index" of "r7" is 7
    And the property "created" of "root" is at most 12

  Scenario: Turning reuse off again destroys what was kept
    When the property "reuseItems" of "list" is set to true
    And the property "contentY" of "list" is set to 40
    And 100 milliseconds pass
    And the property "reuseItems" of "list" is set to false
    And the property "contentY" of "list" is set to 1000
    And 100 milliseconds pass
    Then the property "created" of "root" is at least 10
