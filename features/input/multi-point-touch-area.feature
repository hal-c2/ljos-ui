# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-multipointtoucharea.html
#   https://doc.qt.io/qt-6/qml-qtquick-touchpoint.html
#   https://doc.qt.io/qt-6/qml-qtquick-gestureevent.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: MultiPointTouchArea follows every finger on it
  A MultiPointTouchArea tells what each finger on it does, each as a
  TouchPoint: `pressed` for the points that came down, `updated` for
  those still down, `released` for those lifted, then `touchUpdated`
  with every point down. `gestureStarted` comes first while a point is
  further than the drag threshold from where it came down. It takes an
  event only while the fingers in it are from `minimumTouchPoints` to
  `maximumTouchPoints`, and the mouse is one more point while
  `mouseEnabled`.

  The log tells each signal by its letter, then each point as its `x`,
  its `y` and `d` while it is down or `u` once it is up.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 300; height: 200
        property string log: ""
        function ps(points) {
          var s = []
          for (var i = 0; i < points.length; i++)
            s.push(points[i].x + "," + points[i].y + (points[i].pressed ? "d" : "u"))
          return s.join("/")
        }
        MouseArea {
          anchors.fill: parent
          onPressed: root.log += "M "
        }
        MultiPointTouchArea {
          objectName: "area"
          anchors.fill: parent
          property int least: 1
          property int most: 99
          minimumTouchPoints: least
          maximumTouchPoints: most
          touchPoints: [ TouchPoint { objectName: "first" } ]
          onPressed: (points) => root.log += "P" + root.ps(points) + " "
          onUpdated: (points) => root.log += "U" + root.ps(points) + " "
          onReleased: (points) => root.log += "R" + root.ps(points) + " "
          onCanceled: (points) => root.log += "C" + root.ps(points) + " "
          onTouchUpdated: (points) => root.log += "T" + root.ps(points) + " "
          onGestureStarted: (g) => root.log += "G" + g.touchPoints.length + " "
        }
      }
      """

  Scenario: An area takes any number of points, and the mouse, until it is told otherwise
    Then the property "minimumTouchPoints" of "area" is 1
    And the property "mouseEnabled" of "area" is true
    And the property "pressed" of "first" is false
    And the property "pointId" of "first" is 0

  Scenario: One finger is pressed, updated as it moves and released
    When the user's fingers go through the frames:
      """
      0 at 50, 50
      0 at 60, 50
      0 at 80, 50
      """
    Then the property "log" of "root" is "P50,50d T50,50d G1 U60,50d T60,50d G1 U80,50d T80,50d R80,50u T "

  Scenario: The first finger has the first declared TouchPoint, which keeps where it was
    When the user's fingers go through the frames:
      """
      0 at 50, 40
      0 at 70, 45
      """
    Then the property "pressed" of "first" is false
    And the property "x" of "first" is 70
    And the property "y" of "first" is 45
    And the property "startX" of "first" is 50
    And the property "sceneY" of "first" is 45

  Scenario: A finger a little way from where it came down starts no gesture
    When the user's fingers go through the frames:
      """
      0 at 50, 50
      0 at 56, 56
      0 at 58, 58
      """
    Then the property "log" of "root" is "P50,50d T50,50d U56,56d T56,56d U58,58d T58,58d R58,58u T "

  Scenario: Two fingers are each told, a still one among those updated
    When the user's fingers go through the frames:
      """
      0 at 50, 50
      0 at 50, 50; 1 at 150, 50
      0 at 50, 50; 1 at 152, 50
      1 at 152, 50
      """
    Then the property "log" of "root" is "P50,50d T50,50d U50,50d P150,50d T50,50d/150,50d U50,50d/152,50d T50,50d/152,50d R50,50u U152,50d T152,50d R152,50u T "

  Scenario: Fingers fewer than the minimum are not told
    When the property "least" of "area" is set to 2
    And the user's fingers go through the frames:
      """
      0 at 50, 50
      0 at 52, 50
      0 at 52, 50; 1 at 150, 50
      0 at 54, 50; 1 at 150, 50
      1 at 150, 50
      """
    Then the property "log" of "root" is "P52,50d/150,50d T52,50d/150,50d U54,50d/150,50d T54,50d/150,50d R54,50u U150,50d T150,50d "

  Scenario: Fingers more than the maximum are not told
    When the property "most" of "area" is set to 1
    And the user's fingers go through the frames:
      """
      0 at 50, 50
      0 at 50, 50; 1 at 150, 50
      0 at 52, 50; 1 at 152, 50
      0 at 54, 50
      """
    Then the property "log" of "root" is "P50,50d T50,50d R54,50u T "

  Scenario: The mouse is one more point
    When the user presses the pointer at 50, 50
    And the user moves the pointer to 70, 60
    And the user releases the pointer at 70, 60
    Then the property "log" of "root" is "P50,50d T50,50d G1 U70,60d T70,60d R70,60u T "

  Scenario: Without mouseEnabled the mouse goes to what is under the area
    When the property "mouseEnabled" of "area" is set to false
    And the user presses the pointer at 50, 50
    And the user releases the pointer at 50, 50
    Then the property "log" of "root" is "M "
