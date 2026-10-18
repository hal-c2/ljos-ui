# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html#pixelAligned-prop
#   docs/ARCHITECTURE.md §1 Goals (frame rate on a software rasteriser)
# Qt draws moving content where it is, between pixels too; Ljós draws it
# on whole pixels while it moves, so that a scrolled frame can move what
# the window shows and draw only what comes into view.

@ljos-only
Feature: The content of a moving Flickable is drawn on whole pixels
  While a Flickable moves (dragged, flicked or wheeled) its content is
  drawn at the nearest whole device pixel. Its contentX and contentY,
  and where its items are, stay what they are; only the picture is
  rounded. Once it rests, the content is drawn exactly where it is.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 200
        color: "white"
        Flickable {
          objectName: "f"
          anchors.fill: parent
          contentWidth: 200; contentHeight: 1000
          boundsBehavior: Flickable.StopAtBounds
          Rectangle { width: 200; height: 100; color: "red"; antialiasing: true }
        }
      }
      """

  Scenario: Content that moves is drawn on the nearest whole pixel
    When the user presses the pointer at 100, 150
    And the user moves the pointer to 100, 130
    And the user moves the pointer to 100, 120
    And the property "contentY" of "f" is set to 0.25
    Then the property "moving" of "f" is true
    And the property "contentY" of "f" is 0.25
    And the pixel at 50, 99 of the window is "#ff0000"
    And the pixel at 50, 100 of the window is "#ffffff"

  Scenario: Content at rest is drawn where it is
    When the property "contentY" of "f" is set to 0.25
    Then the property "moving" of "f" is false
    And the pixel at 50, 99 of the window is within 40 of "#ff4040"

  Scenario: Content that comes to rest is drawn where it is again
    When the user presses the pointer at 100, 150
    And the user moves the pointer to 100, 130
    And the user moves the pointer to 100, 120
    And the property "contentY" of "f" is set to 0.25
    And 500 milliseconds pass
    And the user releases the pointer at 100, 120
    And 100 milliseconds pass
    Then the property "moving" of "f" is false
    And the pixel at 50, 99 of the window is within 40 of "#ff4040"
