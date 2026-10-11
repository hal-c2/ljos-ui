# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (contentHeight is an estimate when rows differ)
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html (originY)
#   docs/ARCHITECTURE.md section 8.4
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: The length of a list whose rows differ in height
  The view measures the rows it has made and estimates the rest from their average.

  Background:
    Given the QML:
      """qml
      import QtQuick
      ListView {
        objectName: "list"
        width: 200; height: 300
        model: 1000
        delegate: Item {
          objectName: "r" + index; width: 200
          height: [20, 30, 40][index % 3]
        }
      }
      """
    When 100 milliseconds pass

  @qt-parity
  Scenario: Rows of different heights follow one another
    Then the property "y" of "r1" is 20
    And the property "y" of "r2" is 50
    And the property "y" of "r3" is 90
    And the property "y" of "r6" is 180

  @qt-parity
  Scenario: The length is close to the number of rows times the average height
    Then the property "contentHeight" of "list" is within 1000 of 30000
    And the property "originY" of "list" is 0

  @qt-parity
  Scenario: After scrolling the rows in the view still follow one another
    When the property "contentY" of "list" is set to 900
    And 100 milliseconds pass
    Then the property "y" of "r30" is 900
    And the property "y" of "r31" is 920
    And the property "y" of "r33" is 990
    And the property "contentHeight" of "list" is within 1500 of 30000

  @qt-parity
  Scenario: The end of the list is where the last row ends
    When the method "positionViewAtEnd" of "list" is called
    And 100 milliseconds pass
    Then the property "atYEnd" of "list" is true
    And the object "r999" exists
    And the property "contentHeight" of "list" is within 1500 of 30000

  @qt-parity
  Scenario: A row keeps its place when the view comes back to it
    When the property "contentY" of "list" is set to 900
    And 100 milliseconds pass
    And the property "contentY" of "list" is set to 12000
    And 100 milliseconds pass
    And the property "contentY" of "list" is set to 900
    And 100 milliseconds pass
    Then the property "y" of "r30" is 900
    And the property "y" of "r33" is 990
