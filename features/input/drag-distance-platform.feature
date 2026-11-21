# Sources:
#   https://developer.android.com/reference/android/view/ViewConfiguration#getScaledTouchSlop()
#   https://doc.qt.io/qt-6/qstylehints.html#startDragDistance-prop
#   features/input/drag-threshold.feature (the same drags at the default 10)
# A platform says how far a finger goes before a press is a drag: Android
# calls it the touch slop, 8 dp scaled to the screen. Everything that
# starts a drag starts from what it says, not from a desktop's 10. Every
# move here is a pixel or two at a time, so that the move that starts a
# drag is known.
@ljos-only
Feature: The platform's drag distance starts every drag
  When the system says a drag starts sooner or later than 10 pixels,
  `Qt.styleHints.startDragDistance` says so, and a DragHandler, a
  Flickable, a MultiPointTouchArea and a PinchArea all start from it.

  Background:
    Given the system setting "drag distance" is 8
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 400
        property real distance: Qt.styleHints.startDragDistance
        property string log: ""
        Rectangle {
          objectName: "box"
          width: 100; height: 100
          DragHandler { objectName: "handler" }
        }
        Flickable {
          objectName: "f"
          x: 200; width: 200; height: 200
          contentWidth: 200; contentHeight: 2000
        }
        MultiPointTouchArea {
          y: 200; width: 200; height: 200
          onGestureStarted: (g) => root.log += "G "
        }
        PinchArea {
          objectName: "pinch"
          x: 200; y: 200; width: 200; height: 200
          onPinchStarted: root.log += "S "
        }
      }
      """

  Scenario: The system's distance is what the application is told
    Then the property "distance" of "root" is 8

  Scenario: A DragHandler starts at the first move past the system's distance
    When the user presses the pointer at 50, 50
    And the user moves the pointer to 50, 46
    And the user moves the pointer to 50, 42
    Then the property "active" of "handler" is false
    When the user moves the pointer to 50, 41
    Then the property "active" of "handler" is true

  Scenario: A Flickable starts at the move after the first past the system's distance
    When the user presses the pointer at 300, 100
    And the user moves the pointer to 300, 96
    And the user moves the pointer to 300, 92
    And the user moves the pointer to 300, 91
    Then the property "dragging" of "f" is false
    When the user moves the pointer to 300, 90
    Then the property "dragging" of "f" is true

  Scenario: A finger past the system's distance starts a gesture
    When the user's fingers go through these positions:
      """
      0 at 50, 250
      0 at 50, 255
      0 at 50, 259
      """
    Then the property "log" of "root" is "G "

  Scenario: Two fingers that come nine pixels nearer start a pinch
    When the user's fingers go through these positions:
      """
      0 at 250, 300
      0 at 250, 300; 1 at 350, 300
      0 at 254, 300; 1 at 345, 300
      """
    Then the property "log" of "root" is "S "
