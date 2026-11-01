# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-models-listmodel.html
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity @headless
Feature: A delegate whose row left the model does not read its neighbours
  A delegate that reads the row before it does so while it has a row. When
  the model shrinks, its index is -1 by the time its bindings run again, so
  a guard on the index holds and nothing reads a row that has gone. Each
  test below prints nothing more than its result.

  Scenario Outline: A list of rows that read the row before them
    Given the file "tst_shrink.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Shrink"; when: windowShown; width: 300; height: 300; visible: true
        ListModel { id: blocks }
        function fill(rows) { blocks.clear(); for (const r of rows) blocks.append(r) }
        Component { id: row
          Item {
            required property int index
            width: 300; height: 20
            property string previousId: index > 0 ? blocks.get(index - 1).id : ""
          }
        }
        Component { id: shown; ListView { width: 300; height: 150; model: blocks; delegate: row } }
        function test_change() {
          fill([{ id: "a" }, { id: "b" }, { id: "c" }])
          createTemporaryObject(shown, this)
          wait(20)
          <change>
          wait(20)
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Shrink::initTestCase()
      PASS   : Shrink::test_change()
      PASS   : Shrink::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

    Examples:
      | change                           |
      | fill([{ id: "x" }, { id: "y" }]) |
      | blocks.clear()                   |
      | blocks.remove(2)                 |
      | blocks.remove(0)                 |
      | blocks.remove(1, 2)              |

  Scenario Outline: A repeater of rows that read the row before them
    Given the file "tst_shrink.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Shrink"; when: windowShown; width: 300; height: 300; visible: true
        ListModel { id: blocks }
        function fill(rows) { blocks.clear(); for (const r of rows) blocks.append(r) }
        Component { id: row
          Item {
            required property int index
            width: 300; height: 20
            property string previousId: index > 0 ? blocks.get(index - 1).id : ""
          }
        }
        Component { id: shown; Column { Repeater { model: blocks; delegate: row } } }
        function test_change() {
          fill([{ id: "a" }, { id: "b" }, { id: "c" }])
          createTemporaryObject(shown, this)
          wait(20)
          <change>
          wait(20)
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Shrink::initTestCase()
      PASS   : Shrink::test_change()
      PASS   : Shrink::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

    Examples:
      | change                           |
      | fill([{ id: "x" }, { id: "y" }]) |
      | blocks.clear()                   |
      | blocks.remove(2)                 |
      | blocks.remove(0)                 |
      | blocks.remove(1, 2)              |
