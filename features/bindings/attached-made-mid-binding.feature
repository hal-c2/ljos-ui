# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qml-qtquick-screen.html
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html#property-change-signal-handlers
# Verified on Qt 6.12.0 (qmltestrunner, offscreen).

@qt-parity
Feature: An attached object made while a binding works out its value
  A binding that is the first to read an attached object, such as
  `Screen`, makes that object while it works out its value. A change
  handler waiting to be told of the same change waits until the binding
  has its value: run inside it, the handler would change what the binding
  is reading, and the binding would be its own cause. A test file runs
  as an application does, working a binding out when it is first read,
  so the handler that first reads it is where the attached object is made.

  Scenario: A handler queued behind the one that makes the attached object runs after it
    Given the file "tst_handlers.qml":
      """qml
      import QtQuick
      import QtTest
      Item {
        id: root
        property int go: 0
        property int cols: 0
        property real seen: -1
        readonly property real scale: go > 0 ? (cols > 0 ? 2 : 1) / Screen.devicePixelRatio : 0
        property QtObject first: QtObject {
          property int at: root.go
          onAtChanged: root.seen = root.scale
        }
        property QtObject second: QtObject {
          property int at: root.go
          onAtChanged: root.cols = 33
        }
        TestCase { name: "Handlers"
          function test_order() {
            root.go = 1
            compare(root.seen, 1)
            compare(root.cols, 33)
            compare(root.scale, 2)
          }
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Handlers::initTestCase()
      PASS   : Handlers::test_order()
      PASS   : Handlers::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """
