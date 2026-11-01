# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#exit-prop
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity @headless
Feature: A menu on its way out leaves the keyboard where its item put it
  A menu item can move the keyboard somewhere else. A menu with an exit
  transition is still there while that runs, and does not take it back.

  Scenario: An item moves the keyboard to a field and the menu fades out
    Given the file "tst_menu.qml":
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      import QtTest
      TestCase {
        id: t; name: "MenuFocus"; when: windowShown; width: 600; height: 400; visible: true
        Component { id: view
          Item { width: 600; height: 400
            property alias menu: menu
            TextField { id: field; objectName: "field"; width: 100 }
            Menu { id: menu; x: 200; y: 100
              exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 90 } }
              MenuItem { objectName: "item"; text: "go"; onTriggered: field.forceActiveFocus() }
            }
          }
        }
        function test_focus() {
          const v = createTemporaryObject(view, t); v.menu.popup(); tryVerify(() => v.menu.opened)
          mouseClick(findChild(v, "item"))
          verify(findChild(v, "field").activeFocus, "right after the click")
          tryVerify(() => !v.menu.visible); wait(50)
          verify(findChild(v, "field").activeFocus, "after the menu closed")
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : MenuFocus::initTestCase()
      PASS   : MenuFocus::test_focus()
      PASS   : MenuFocus::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """
