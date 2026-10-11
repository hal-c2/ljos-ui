# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-taphandler.html#point-prop
#   https://doc.qt.io/qt-6/qml-qtquick-handlerpoint.html#pressedButtons-prop
#   https://doc.qt.io/qt-6/qml-qtquick-taphandler.html#gesturePolicy-prop
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity
Feature: The buttons and the bounds of a TapHandler's point
  The point names the button that is held, whichever the handler takes,
  and a handler that gives the press up when the pointer leaves its item
  empties its point there.

  Scenario: The point names the button that is held
    Given the file "tst_button.qml":
      """qml
      import QtQuick
      import QtTest
      Item {
        id: box
        width: 200; height: 200
        TapHandler { id: handler; acceptedButtons: Qt.LeftButton | Qt.RightButton }
        TestCase {
          name: "Button"
          when: windowShown
          function test_button() {
            mousePress(box, 10, 30, Qt.RightButton)
            compare(handler.point.pressedButtons, Qt.RightButton)
            mouseRelease(box, 10, 30, Qt.RightButton)
            compare(handler.point.pressedButtons, Qt.NoButton)
          }
        }
      }
      """
    When the tests are run
    Then the test report has the lines:
      """
      PASS   : Button::test_button()
      """

  Scenario: A handler that stays within its bounds empties its point outside them
    Given the file "tst_bounds.qml":
      """qml
      import QtQuick
      import QtTest
      Item {
        width: 200; height: 200
        Item {
          id: box
          width: 100; height: 100
          TapHandler { id: handler; gesturePolicy: TapHandler.WithinBounds }
        }
        TestCase {
          name: "Bounds"
          when: windowShown
          function test_bounds() {
            mousePress(box, 10, 30)
            mouseMove(box, 80, 33)
            compare(handler.point.position, Qt.point(80, 33))
            mouseMove(box, 120, 33)
            compare([handler.pressed, handler.point.id], [false, -1])
            compare(handler.point.position, Qt.point(0, 0))
          }
        }
      }
      """
    When the tests are run
    Then the test report has the lines:
      """
      PASS   : Bounds::test_bounds()
      """
