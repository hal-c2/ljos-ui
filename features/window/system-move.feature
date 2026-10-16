# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#startSystemMove-method
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#startSystemResize-method
#   https://doc.qt.io/qt-6/qt.html#Edge-enum
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity
Feature: A window asks the window system to move or resize it
  A window without a frame of the system's (a title bar of its own) asks
  the window system to start moving it, or resizing it by some of its
  edges, from the press that is going on. Each says whether the system
  took it on: where there is no window system that does, nothing happens.

  @api-QtQuick.Window.startSystemMove @api-QtQuick.Window.startSystemResize
  Scenario: A window can be asked to start moving or resizing
    Given the file "tst_system.qml":
      """qml
      import QtQuick
      import QtTest
      Item {
        Window {
          id: w; width: 10; height: 10
          Item { id: inside }
        }
        TestCase {
          name: "System"
          when: windowShown
          function test_methods() {
            compare(typeof w.startSystemMove, "function")
            compare(typeof w.startSystemResize, "function")
          }
          function test_window() {
            compare(typeof inside.Window.window.startSystemMove, "function")
          }
        }
      }
      """
    When the tests are run
    Then the test report has the lines:
      """
      PASS   : System::test_methods()
      PASS   : System::test_window()
      """

  Scenario Outline: Without a window system that moves windows, nothing is started
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "w"
        width: 100; height: 100
        visible: true
        property var started: null
        function move() { started = startSystemMove() }
        function resize() { started = startSystemResize(Qt.TopEdge | Qt.LeftEdge) }
      }
      """
    When the method "<method>" of "w" is called
    Then the property "started" of "w" is false

    Examples:
      | method |
      | move   |
      | resize |
