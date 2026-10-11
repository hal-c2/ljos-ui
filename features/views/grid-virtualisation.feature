# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-gridview.html (cacheBuffer, header, footer)
#   docs/ARCHITECTURE.md section 8.4
# Verified on Qt 6.11.2 (qml6, offscreen).
# Delegate counts are bands: Qt does not promise exact numbers.

@qt-parity @api-QtQuick.GridView.cacheBuffer
Feature: A GridView has delegates only for the cells near what it shows

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 250; height: 200
        property int created: 0
        property int destroyed: 0
        readonly property int live: created - destroyed
        GridView {
          objectName: "grid"
          anchors.fill: parent
          cellWidth: 80; cellHeight: 60
          model: 30000
          header: Item { objectName: "head"; width: 250; height: 12 }
          footer: Item { objectName: "foot"; width: 250; height: 9 }
          delegate: Item {
            objectName: "g" + index; width: 70; height: 50
            Component.onCompleted: root.created++
            Component.onDestruction: root.destroyed++
          }
        }
      }
      """
    When 200 milliseconds pass

  Scenario: Thirty thousand cells have a few delegates
    Then the property "live" of "root" is at least 12
    And the property "live" of "root" is at most 40
    And the property "contentHeight" of "grid" is 600021

  Scenario: The header is above the first line and the view starts at it
    Then the property "y" of "head" is -12
    And the property "y" of "g0" is 0
    And the property "contentY" of "grid" is -12
    And the property "originY" of "grid" is -12

  Scenario: Scrolling far away does not pile delegates up
    When the property "contentY" of "grid" is set to 300000
    And 200 milliseconds pass
    Then the property "live" of "root" is at most 60
    And the property "y" of "g15000" is 300000
    And the property "x" of "g15001" is 80
    And the object "g5" does not exist

  Scenario: The footer is below the last line
    When the method "positionViewAtEnd" of "grid" is called
    And 200 milliseconds pass
    Then the property "y" of "foot" is 600000
    And the property "atYEnd" of "grid" is true
    And the object "g29999" exists

  Scenario: Rows added to the model fill the next cells
    When the property "model" of "grid" is set to 4
    And 100 milliseconds pass
    Then the property "contentHeight" of "grid" is 141
    And the property "y" of "foot" is 120
    And the property "live" of "root" is 4
