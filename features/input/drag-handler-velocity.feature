# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-draghandler.html#centroid-prop
#   https://doc.qt.io/qt-6/qml-qtquick-handlerpoint.html#velocity-prop
# Verified on Qt 6.11.2 (qml6, qmltestrunner). How fast qml6 says the
# pointer went depends on when its events came, so only the way it went is
# checked here.

@qt-parity
Feature: How fast a DragHandler's pointer goes
  The centroid of a DragHandler says how fast, in pixels a second, the
  pointer that drags it goes. The handler says it is no longer active
  while the centroid still holds that speed, so a script can throw the
  target on; then the centroid empties and is at rest.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property string thrown: ""
        function way(v) {
          return (v.x > 0 ? "right" : v.x < 0 ? "left" : "still") + " "
               + (v.y > 0 ? "down" : v.y < 0 ? "up" : "still")
        }
        Rectangle {
          width: 100; height: 100
          DragHandler {
            id: handler
            objectName: "handler"
            onActiveChanged: if (!active) root.thrown = root.way(centroid.velocity)
          }
        }
        property string rest: way(handler.centroid.velocity)
      }
      """

  Scenario: The speed at the end of a drag goes the way the pointer went
    When the user presses the pointer at 10, 80
    And 20 milliseconds pass
    And the user moves the pointer to 40, 60
    And 20 milliseconds pass
    And the user moves the pointer to 70, 40
    And 20 milliseconds pass
    And the user moves the pointer to 100, 20
    And the user releases the pointer at 100, 20
    Then the property "thrown" of "root" is "right up"
    And the property "rest" of "root" is "still still"

  Scenario: The pointer is at rest before it drags
    When the user presses the pointer at 10, 80
    Then the property "rest" of "root" is "still still"
