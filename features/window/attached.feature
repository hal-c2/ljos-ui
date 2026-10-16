# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#attached-properties
# Verified on Qt 6.11.2 (qml6 and qmltestrunner, offscreen).

@qt-parity
Feature: What an item can ask about the window it is in
  The attached Window names the item's window and its content item, and
  says how large the window is and how it is shown. The values follow the
  window as it changes.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Window {
        id: w
        objectName: "w"
        width: 300; height: 200
        visible: true
        Item {
          objectName: "probe"
          property bool isWindow: Window.window === w
          property bool isContent: Window.contentItem === w.contentItem
          property real windowWidth: Window.width
          property real windowHeight: Window.height
          property int visibility: Window.visibility
        }
      }
      """

  @api-QtQuick.Window.window @api-QtQuick.Window.contentItem
  Scenario Outline: An item names the window it is in
    Then the property "<property>" of "probe" is true

    Examples:
      | property  |
      | isWindow  |
      | isContent |

  @api-QtQuick.Window.width @api-QtQuick.Window.height @api-QtQuick.Window.visibility
  Scenario Outline: An item reads the size and the visibility of its window
    Then the property "<property>" of "probe" is <value>

    Examples:
      | property     | value |
      | windowWidth  | 300   |
      | windowHeight | 200   |
      | visibility   | 2     |

  @api-QtQuick.Window.visibility
  Scenario: The visibility an item reads follows the window
    When the method "hide" of "w" is called
    Then the property "visibility" of "probe" is 0

  Scenario: An item outside any Window still has a window
    Given the file "tst_outside.qml":
      """qml
      import QtQuick
      import QtTest
      Item {
        id: root
        width: 300; height: 200
        TestCase {
          name: "Outside"
          when: windowShown
          function test_window() {
            verify(root.Window.window !== null && root.Window.window !== undefined)
            verify(root.Window.contentItem !== null && root.Window.contentItem !== undefined)
            compare(root.Window.width, 300)
            compare(root.Window.height, 200)
            compare(root.Window.visibility, Window.Windowed)
            verify(root.Window.active !== undefined)
            verify(root.Window.activeFocusItem !== undefined)
          }
        }
      }
      """
    When the tests are run
    Then the test report has the lines:
      """
      PASS   : Outside::test_window()
      """

  @api-QtQuick.Window.width
  Scenario: The size an item reads follows its window
    Given the file "tst_follows.qml":
      """qml
      import QtQuick
      import QtTest
      Item {
        Component {
          id: made
          Window { width: 300; height: 200; visible: true
            property alias probe: probe
            Item { id: probe; property real windowWidth: Window.width } }
        }
        TestCase {
          name: "Follows"
          when: windowShown
          function test_follows() {
            const w = createTemporaryObject(made, null)
            tryCompare(w.probe, "windowWidth", 300)
            w.width = 420
            tryCompare(w.probe, "windowWidth", 420)
          }
        }
      }
      """
    When the tests are run
    Then the test report has the lines:
      """
      PASS   : Follows::test_follows()
      """
