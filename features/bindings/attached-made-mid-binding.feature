# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qml-qtquick-screen.html
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html#property-change-signal-handlers
# Verified on Qt 6.12.0 (qmltestrunner, offscreen, QT_SCALE_FACTOR 1 and 2).

@qt-parity
Feature: An attached object made while a binding works out its value
  A binding that is the first to read an attached object, such as
  `Screen`, makes that object while it works out its value. A change
  handler waiting to be told of a change waits until the binding has its
  value: run inside it, the handler would change what the binding has
  already read, and the binding would be its own cause. A test file runs
  as an application does, working a binding out when it is first read.

  Scenario: A handler queued while a component is made waits for the binding that makes Screen
    # The scroll bar is the first to read the stage's width, which has
    # read `cols` before `scale` makes the page's `Screen`; the handler
    # that sets `cols` runs after.
    Given the file "tst_made.qml":
      """qml
      import QtQuick
      import QtQuick.Controls
      import QtTest
      Item {
        id: root; width: 400; height: 300
        Component { id: maker
          Item { id: page
            readonly property var office: ({})[""] ?? null
            property int cols: 0
            readonly property real scale: cols > 0 ? 2 / Screen.devicePixelRatio : 1
            readonly property real stageWidth: stage.width
            onOfficeChanged: cols = 40
            Flickable { anchors.fill: parent; contentWidth: stage.width
              ScrollBar.horizontal: ScrollBar {}
              Item { id: stage; width: page.cols * page.scale } } } }
        Loader { id: loader; anchors.fill: parent }
        TestCase { name: "Made"; when: windowShown
          function test_made() {
            loader.sourceComponent = maker
            compare(loader.item.cols, 40)
            compare(loader.item.stageWidth, 80 / Screen.devicePixelRatio)
          }
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Made::initTestCase()
      PASS   : Made::test_made()
      PASS   : Made::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """
