# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#contentItem-prop
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity
Feature: A window made by a script lays out what is in it
  A window made with createObject, with no parent, is a window of its own.
  Its items are laid out against its content item as in a window written
  in the file: an item that fills its parent is as large as the window,
  once the window has been laid out (not yet when createObject returns).

  Background:
    Given the file "Made.qml":
      """qml
      import QtQuick
      Window {
        width: 480; height: 320
        visible: true
        property alias fill: fill
        Rectangle { id: fill; anchors.fill: parent }
      }
      """

  @api-QtQuick.Window.contentItem
  Scenario: An item anchored to fill a made window is as large as the window
    Given the file "tst_made.qml":
      """qml
      import QtQuick
      import QtTest
      Item {
        width: 200; height: 100
        TestCase {
          name: "Made"
          when: windowShown
          function test_fill() {
            const w = createTemporaryObject(Qt.createComponent("Made.qml"), null)
            tryCompare(w.fill, "width", 480)
            tryCompare(w.fill, "height", 320)
          }
        }
      }
      """
    When the tests are run
    Then the test report has the lines:
      """
      PASS   : Made::test_fill()
      """

  Scenario: An item declared in the same file fills the window made of it
    Given the file "tst_inline.qml":
      """qml
      import QtQuick
      import QtTest
      Item {
        width: 200; height: 100
        Component {
          id: made
          Window { width: 480; height: 320; visible: true
            property alias fill: fill
            Rectangle { id: fill; anchors.fill: parent } }
        }
        TestCase {
          name: "Inline"
          when: windowShown
          function test_fill() {
            const w = createTemporaryObject(made, null)
            tryCompare(w.fill, "width", 480)
          }
        }
      }
      """
    When the tests are run
    Then the test report has the lines:
      """
      PASS   : Inline::test_fill()
      """

  Scenario: The anchored item follows when the made window is resized
    Given the file "tst_resize.qml":
      """qml
      import QtQuick
      import QtTest
      Item {
        width: 200; height: 100
        TestCase {
          name: "Resize"
          when: windowShown
          function test_resize() {
            const w = createTemporaryObject(Qt.createComponent("Made.qml"), null)
            tryCompare(w.fill, "width", 480)
            w.width = 300
            tryCompare(w.fill, "width", 300)
          }
        }
      }
      """
    When the tests are run
    Then the test report has the lines:
      """
      PASS   : Resize::test_resize()
      """
