# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (cacheBuffer, displayMarginBeginning, displayMarginEnd)
#   docs/ARCHITECTURE.md section 8.4
# Verified on Qt 6.11.2 (qml6, offscreen).
# Delegate counts are bands: Qt does not promise exact numbers.

@qt-parity @api-QtQuick.ListView.cacheBuffer
Feature: A ListView has delegates only for the rows near what it shows
  A long list costs what is visible, not what it holds.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 200; height: 300
        property int created: 0
        property int destroyed: 0
        readonly property int live: created - destroyed
        ListView {
          objectName: "list"
          anchors.fill: parent
          model: 100000
          delegate: Item {
            objectName: "r" + index; width: 200; height: 30
            Component.onCompleted: root.created++
            Component.onDestruction: root.destroyed++
          }
        }
      }
      """
    When 200 milliseconds pass

  Scenario: A hundred thousand rows have a few delegates
    Then the property "count" of "list" is 100000
    And the property "live" of "root" is at least 10
    And the property "live" of "root" is at most 24
    And the property "contentHeight" of "list" is 3000000

  Scenario: The rows in the view and the ones the cache holds exist, the rest do not
    Then the object "r9" exists
    And the object "r15" exists
    And the object "r40" does not exist

  Scenario: Scrolling far away does not pile delegates up
    When the property "contentY" of "list" is set to 1500000
    And 200 milliseconds pass
    Then the property "live" of "root" is at least 10
    And the property "live" of "root" is at most 36
    And the property "y" of "r50000" is 1500000
    And the object "r50005" exists
    And the object "r9" does not exist

  Scenario: Scrolling step by step keeps the number of delegates bounded
    When the property "contentY" of "list" is set to 600
    And 100 milliseconds pass
    And the property "contentY" of "list" is set to 1200
    And 100 milliseconds pass
    And the property "contentY" of "list" is set to 1800
    And 100 milliseconds pass
    Then the property "live" of "root" is at most 36
    And the property "y" of "r60" is 1800
    And the object "r20" does not exist

  Scenario: Without a cache only the rows in the view exist
    When the property "cacheBuffer" of "list" is set to 0
    And the property "contentY" of "list" is set to 3000
    And 200 milliseconds pass
    Then the property "live" of "root" is at least 10
    And the property "live" of "root" is at most 14
    And the object "r100" exists
    And the object "r109" exists
    And the object "r113" does not exist

  Scenario: A display margin shows rows beyond the edges of the view
    When the property "cacheBuffer" of "list" is set to 0
    And the property "displayMarginBeginning" of "list" is set to 90
    And the property "displayMarginEnd" of "list" is set to 90
    And the property "contentY" of "list" is set to 3000
    And 200 milliseconds pass
    Then the object "r98" exists
    And the object "r112" exists
    And the object "r94" does not exist
    And the object "r116" does not exist

  Scenario: The end of a long list can be reached
    When the method "positionViewAtEnd" of "list" is called
    And 200 milliseconds pass
    Then the property "atYEnd" of "list" is true
    And the object "r99999" exists
    And the property "live" of "root" is at most 36
