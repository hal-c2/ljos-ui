# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html#contentY-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#clip-prop
#   docs/ARCHITECTURE.md (drawing the window: the pixels of a clip that
#   scrolled are moved on screen, not drawn again)

# What a scroll shows is Qt's; that its pixels are moved rather than drawn
# again, and so could land in the wrong place, is Ljós's own.
@ljos-only @desktop
Feature: A list scrolled in a clip shows each row where it now is
  A list that clips shows only the part of its content in its bounds.
  After a scroll each row is drawn where the scroll put it, the rows that
  came into sight are drawn in full, and nothing of the list is drawn
  outside its bounds, however many small scrolls came before.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 200; color: "#000080"
        ListView {
          objectName: "list"
          x: 20; y: 20; width: 160; height: 120
          clip: true
          model: 20
          delegate: Rectangle {
            width: 160; height: 40
            color: ["#ff0000", "#00ff00", "#0000ff", "#ffff00", "#ff00ff"][index % 5]
          }
        }
      }
      """

  Scenario: One scroll moves the rows up by as much
    When the property "contentY" of "list" is set to 10
    Then the pixel at 100, 25 of the window is "#ff0000"
    And the pixel at 100, 55 of the window is "#00ff00"
    And the pixel at 100, 135 of the window is "#ffff00"
    And the pixel at 100, 150 of the window is "#000080"
    And the pixel at 100, 10 of the window is "#000080"

  Scenario: Small scrolls one after another, down and back, land where one would
    When the property "contentY" of "list" is set to 7
    And the property "contentY" of "list" is set to 19
    And the property "contentY" of "list" is set to 33
    And the property "contentY" of "list" is set to 61
    Then the pixel at 100, 21 of the window is "#00ff00"
    And the pixel at 100, 100 of the window is "#ffff00"
    And the pixel at 100, 139 of the window is "#ff00ff"
    And the pixel at 100, 141 of the window is "#000080"
    When the property "contentY" of "list" is set to 23
    Then the pixel at 100, 21 of the window is "#ff0000"
    And the pixel at 100, 45 of the window is "#00ff00"
    And the pixel at 100, 139 of the window is "#ffff00"
    And the pixel at 10, 100 of the window is "#000080"
