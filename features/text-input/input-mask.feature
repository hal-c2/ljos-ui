# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#inputMask-prop
#   https://doc.qt.io/qt-6/qlineedit.html#inputMask-prop
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity @headless
Feature: A TextInput's input mask
  The mask says what each place of the field may hold. Its separators are
  part of the text, a place left empty shows the blank character and is not
  part of the text, and the field is acceptable once every place that must
  be filled is.

  Scenario: The mask from the issue
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const time = createTemporaryObject(field, this, { inputMask: "99:99" })
          compare(time.text, ":")
          verify(!time.acceptableInput)
          time.forceActiveFocus(); time.selectAll()
          keyClick("1"); keyClick("2"); keyClick("3"); keyClick("4")
          compare(time.text, "12:34")
          verify(time.acceptableInput)
          time.selectAll(); keyClick("x")
          verify(time.text.indexOf("x") < 0)
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

  Scenario Outline: A field starts with its mask
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "<mask>" })
          compare(i.inputMask.replace(/ $/, "~"), "<read>")
          compare(i.length, <length>)
          compare(i.maximumLength, <max>)
          compare(state(i), "<state>")
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

    Examples:
      | mask | read | length | max | state |
      | 99:99 | 99:99;~ | 5 | 5 | :/  :  /0/false |
      | AA-99;# | AA-99;# | 5 | 5 | -/##-##/0/false |
      | (999) 999;* | (999) 999;* | 9 | 9 | () /(***) ***/1/false |
      | aaa | aaa;~ | 3 | 3 | /   /0/true |
      | 000 | 000;~ | 3 | 3 | /   /0/true |
      | ### | ###;~ | 3 | 3 | /   /0/true |
      | hhh | hhh;~ | 3 | 3 | /   /0/true |
      | bbb | bbb;~ | 3 | 3 | /   /0/true |
      | ddd | ddd;~ | 3 | 3 | /   /0/true |
      | nnn | nnn;~ | 3 | 3 | /   /0/true |
      | xxx | xxx;~ | 3 | 3 | /   /0/true |
      | NNN | NNN;~ | 3 | 3 | /   /0/false |
      | XXX | XXX;~ | 3 | 3 | /   /0/false |
      | DDD | DDD;~ | 3 | 3 | /   /0/false |
      | HHH | HHH;~ | 3 | 3 | /   /0/false |
      | BBB | BBB;~ | 3 | 3 | /   /0/false |
      | >AA<aa!Aa | >AA<aa!Aa;~ | 6 | 6 | /      /0/false |
      | [a] | [a];~ | 1 | 1 | / /0/true |
      | {a} | {a};~ | 1 | 1 | / /0/true |
      | a[b]c | a[b]c;~ | 3 | 3 | c/  c/0/true |
      | 9;ab | 9;a | 1 | 1 | /a/0/false |
      | 9;; | 9;; | 1 | 1 | /;/0/false |
      | 9; | 9;~ | 1 | 1 | / /0/false |
      | --- | ---;~ | 3 | 3 | ---/---/3/true |
      | X;_ | X;_ | 1 | 1 | /_/0/false |
      | < | <;~ | 0 | 0 | //0/true |
      |  |  | 0 | 32767 | //0/true |
      | ;_ |  | 0 | 32767 | //0/true |

  Scenario: A backslash makes a mask character a separator
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const bs = String.fromCharCode(92)
          const i = createTemporaryObject(field, this, { inputMask: bs + "A9" + bs + "9" })
          compare(state(i), "A9/A 9/1/false")
          i.forceActiveFocus(); keyClick("5")
          compare(state(i), "A59/A59/3/true")
          compare(createTemporaryObject(field, this, { inputMask: "9" + bs }).length, 1)
          compare(createTemporaryObject(field, this, { inputMask: bs + bs + "9" }).text, bs)
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

  Scenario Outline: Typing
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "<mask>" })
          i.forceActiveFocus()
          for (const k of "<keys>".replace(/~/g, " ")) keyClick(k)
          compare(state(i), "<state>")
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

    Examples:
      | mask | keys | state |
      | AAAAAAAA | aZ5 -+.3 | aZ/aZ      /2/false |
      | aaaaaaaa | aZ5 -+.3 | aZ/aZ      /3/true |
      | NNNNNNNN | aZ5 -+.3 | aZ53/aZ53    /4/false |
      | nnnnnnnn | aZ5 -+.3 | aZ53/aZ5 3   /5/true |
      | XXXXXXXX | aZ5 -+.3 | aZ5-+.3/aZ5-+.3 /7/false |
      | xxxxxxxx | aZ5 -+.3 | aZ5-+.3/aZ5 -+.3/8/true |
      | 99999999 | aZ5 -+.3 | 53/53      /2/false |
      | 00000000 | aZ5 -+.3 | 53/5 3     /3/true |
      | DDDDDDDD | aZ5 -+.3 | 53/53      /2/false |
      | dddddddd | aZ5 -+.3 | 53/5 3     /3/true |
      | ######## | aZ5 -+.3 | 5-+3/5 -+3   /5/true |
      | HHHHHHHH | aZ5 -+.3 | a53/a53     /3/false |
      | hhhhhhhh | aZ5 -+.3 | a53/a5 3    /4/true |
      | HHHH | fG0a | f0a/f0a /3/false |
      | BB | 012 | 01/01/2/true |
      | DD | 01 | 1/1 /1/false |
      | >AA<AA!AA | abABaB | ABabaB/ABabaB/6/true |
      | 99:99 | 12345 | 12:34/12:34/5/true |
      | 99:99 | 1:2 | 1:2/1 :2 /4/false |
      | 99-AA | 12ab | 12-ab/12-ab/5/true |
      | 99-AA | a | -a/  -a /4/false |
      | A9 | 5 | 5/ 5/2/false |
      | 99:99 | : | :/  :  /3/false |
      | 9-9-9 | --1 | -1-/ -1- /4/false |
      | 9-9-9 | 1-2-3 | 1-2-3/1-2-3/5/true |
      | 9-9-9 | 12- | 1-2-/1-2- /4/false |
      | 9-9+9-9 | -+- | -+-/ - + - /6/false |
      | -A | x | -x/-x/2/true |
      | 9x9 | 1X2 | 1X2/1X2/3/true |
      | a;_ | _ | /_/1/true |
      | a;_ | ~ | /_/0/true |
      | A;_ | _ | /_/0/false |
      | X;_ | _ | /_/0/false |
      | XX;_ | 1~ | 1 /1 /2/true |
      | a b | x~ | x /x  /3/true |
      | X-X | a- | a--/a--/3/true |
      | 9-9;- | 1- | 1-/1--/2/false |
      | a b;_ | x~ | x /x _/2/true |
      | 99;1 | 12 | 2/12/2/true |
      | --- | x | ---/---/3/true |

  Scenario Outline: Assigning the text
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "99:99" })
          i.text = "<value>"
          compare(state(i), "<state>")
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

    Examples:
      | value | state |
      | 1234 | 12:34/12:34/5/true |
      | 12:34 | 12:34/12:34/5/true |
      | 1 | 1:/1 :  /5/false |
      | x1 | 1:/1 :  /5/false |
      | 12:3 | 12:3/12:3 /5/false |
      | 12:345 | 12:34/12:34/5/true |
      | :34 | :34/  :34/5/false |
      |  | :/  :  /5/false |

  Scenario: The arrow keys and Home and End stop at the places
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "(99)-(99);_" })
          compare(i.cursorPosition, 1)
          i.forceActiveFocus()
          const seen = []
          keyClick(Qt.Key_Home); seen.push(i.cursorPosition)
          for (let n = 0; n < 6; n++) { keyClick(Qt.Key_Right); seen.push(i.cursorPosition) }
          for (let n = 0; n < 6; n++) { keyClick(Qt.Key_Left); seen.push(i.cursorPosition) }
          compare(seen.join(), "0,1,2,6,7,9,9,7,6,2,1,0,0")
          keyClick(Qt.Key_End); compare(i.cursorPosition, 6)
          i.text = "12-34"; compare(i.text, "(12)-(34)")
          i.cursorPosition = 0; keyClick(Qt.Key_End); compare(i.cursorPosition, 9)
          i.text = "1"; i.cursorPosition = 9; keyClick(Qt.Key_End); compare(i.cursorPosition, 6)
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

  Scenario: Moving by words stops at the places
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "AA-AA", text: "ab-cd" })
          i.forceActiveFocus(); i.cursorPosition = 5
          const seen = []
          for (let n = 0; n < 3; n++) { keyClick(Qt.Key_Left, Qt.ControlModifier); seen.push(i.cursorPosition) }
          for (let n = 0; n < 2; n++) { keyClick(Qt.Key_Right, Qt.ControlModifier); seen.push(i.cursorPosition) }
          compare(seen.join(), "3,1,0,3,5")
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

  Scenario Outline: A program moves the cursor within the text and off the separators
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "99:99" })
          i.text = "<text>"
          i.cursorPosition = <from>
          i.cursorPosition = <to>
          compare(i.cursorPosition, <cursor>)
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

    Examples:
      | text | from | to | cursor |
      | 12:34 | 0 | 2 | 3 |
      | 12:34 | 1 | 2 | 3 |
      | 12:34 | 4 | 2 | 1 |
      | 12:34 | 5 | 2 | 1 |
      | 12:34 | 0 | 6 | 0 |
      | 12:34 | 1 | -1 | 1 |
      | 12:3 | 0 | 5 | 0 |
      | 12:3 | 0 | 4 | 4 |
      | 1 | 0 | 2 | 3 |
      | 1 | 0 | 3 | 0 |
      |  | 0 | 1 | 1 |
      |  | 0 | 2 | 0 |

  Scenario Outline: Deleting leaves the separators and blanks
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "99:99", text: "12:34" })
          i.forceActiveFocus()
          <action>
          compare(state(i), "<state>")
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

    Examples:
      | action | state |
      | i.cursorPosition = 5; keyClick(Qt.Key_Backspace) | 12:3/12:3 /4/false |
      | i.cursorPosition = 3; keyClick(Qt.Key_Backspace) | 1:34/1 :34/1/false |
      | i.cursorPosition = 0; keyClick(Qt.Key_Backspace) | 12:34/12:34/0/true |
      | i.cursorPosition = 0; keyClick(Qt.Key_Delete) | 2:34/ 2:34/0/false |
      | i.cursorPosition = 5; keyClick(Qt.Key_Delete) | 12:34/12:34/5/true |
      | i.select(1, 4); keyClick(Qt.Key_Delete) | 1:4/1 : 4/1/false |
      | i.select(1, 4); keyClick(Qt.Key_Backspace) | 1:4/1 : 4/1/false |
      | i.select(2, 3); keyClick(Qt.Key_Delete) | 12:34/12:34/2/true |
      | i.select(1, 4); keyClick("9") | 19:4/19: 4/3/false |
      | i.select(1, 4); keyClick("x") | 1:4/1 : 4/1/false |
      | i.select(2, 3); keyClick("7") | 12:74/12:74/4/true |
      | i.cursorPosition = 3; keyClick(Qt.Key_Backspace, Qt.ControlModifier) | 1:34/1 :34/1/false |
      | i.cursorPosition = 3; keyClick(Qt.Key_Delete, Qt.ControlModifier) | 12:/12:  /3/false |
      | i.clear() | :/  :  /0/false |
      | i.remove(1, 4) | 1:4/1 : 4/5/false |
      | i.cursorPosition = 1; keyClick("9") | 19:34/19:34/3/true |
      | i.cursorPosition = 1; keyClick(":") | 12:34/12:34/3/true |
      | i.cursorPosition = 4; keyClick(":") | 12:34/12:34/4/true |
      | i.cursorPosition = 1; i.insert(1, "9") | 19:34/19:34/1/true |
      | i.cursorPosition = 1; i.insert(0, "xx") | 12:34/12:34/1/true |
      | i.cursorPosition = 1; i.insert(0, "77:88") | 77:88/77:88/1/true |

  Scenario: Selecting
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "99:99" })
          i.text = "1"
          i.selectAll()
          compare(i.selectedText, "1 ")
          compare(i.selectionEnd, 2)
          compare(i.getText(0, 3), "1 :")
          i.text = "12:34"
          i.select(3, 2)
          compare(i.cursorPosition, 2)
          compare(i.selectedText, ":")
          i.deselect(); i.cursorPosition = 4; i.moveCursorSelection(2)
          compare(i.cursorPosition, 1)
          compare(i.selectedText, "2:3")
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

  Scenario: The clipboard
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "99:99", text: "12:34" })
          i.select(0, 2); i.cut()
          compare(state(i), ":34/  :34/0/false")
          i.cursorPosition = 3; i.paste()
          compare(state(i), ":12/  :12/5/false")
          i.text = "12:34"; i.select(0, 5); i.copy()
          i.text = ""; i.select(1, 4); i.paste()
          compare(state(i), "1:23/ 1:23/5/false")
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

  Scenario: Undo
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "9999" })
          i.forceActiveFocus()
          for (const k of "1234") keyClick(k)
          i.undo()
          compare(state(i), "/    /0/false")
          verify(!i.canUndo)
          i.redo()
          compare(state(i), "1234/1234/4/true")
          i.text = "12"
          verify(!i.canUndo)
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

  Scenario: Changing the mask reads the text again
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { text: "1234" })
          i.inputMask = "99:99"
          compare(state(i), "12:34/12:34/0/true")
          i.inputMask = ">AA;_"
          compare(state(i), "/__/0/false")
          compare(i.maximumLength, 2)
          i.maximumLength = 7
          compare(i.maximumLength, 2)
          i.text = "ab"
          compare(i.text, "AB")
          i.inputMask = ""
          compare(state(i), "//0/true")
          compare(i.maximumLength, 32767)
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

  Scenario: Writing the same mask is no change
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "9" })
          let changes = 0
          i.inputMaskChanged.connect(() => changes++)
          i.inputMask = "9"
          i.inputMask = "9; "
          compare(changes, 0)
          i.inputMask = "AA;_"
          compare(changes, 1)
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

  Scenario Outline: The characters a place takes, beyond ASCII
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "<mask>" })
          i.insert(0, "<char>")
          compare(i.text, "<text>")
          compare(i.acceptableInput, <ok>)
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

    Examples:
      | mask | char | text | ok |
      | A | é | é | true |
      | A | ª | ª | true |
      | A | 中 | 中 | true |
      | A | Ⅷ |  | false |
      | A | _ |  | false |
      | 9 | ² | ² | true |
      | 9 | ½ | ½ | true |
      | 9 | Ⅷ | Ⅷ | true |
      | 9 | ٠ | ٠ | true |
      | N | ñ | ñ | true |
      | N | ² | ² | true |
      | N | _ |  | false |
      | D | ٠ |  | false |
      | D | ٣ | ٣ | true |
      | D | ½ |  | false |
      | D | ⁰ |  | false |
      | D | ² | ² | true |
      | D | １ | １ | true |
      | D | ０ |  | false |
      | D | Ⅷ |  | false |
      | D | ① | ① | true |
      | D | ⓪ |  | false |
      | B | ٠ |  | false |
      | H | ٣ | ٣ | true |
      | H | F | F | true |
      | H | G |  | false |
      | H | ａ |  | false |
      | # | ² | ² | true |
      | X | 😀 |  | false |
      | >A | é | É | true |
      | >X | ß | ß | true |
      | >A | ǆ | Ǆ | true |
      | <A | Σ | σ | true |
      | <A | İ | İ | true |

  Scenario Outline: Characters that cannot be seen
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "<mask>" })
          i.insert(0, String.fromCharCode(<code>))
          compare(i.text.length, <kept>)
          compare(i.acceptableInput, <ok>)
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

    Examples:
      | mask | code   | kept | ok    |
      | A    | 0x0301 | 0    | false |
      | N    | 0x0301 | 0    | false |
      | X    | 0x0009 | 0    | false |
      | X    | 0x007f | 0    | false |
      | X    | 0x0085 | 0    | false |
      | X    | 0x00a0 | 1    | true  |
      | X    | 0x00ad | 0    | false |
      | X    | 0x0301 | 1    | true  |
      | X    | 0x200b | 0    | false |
      | X    | 0x2028 | 1    | true  |
      | X    | 0x3000 | 1    | true  |
      | X    | 0xe000 | 0    | false |

  Scenario: A separator beyond the Basic Multilingual Plane takes two places
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "9\u{1F600}9" })
          compare(i.length, 4)
          compare(i.text, "\u{1F600}")
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

  Scenario: A validator judges the text without the blanks
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const v = Qt.createQmlObject("import QtQuick; RegularExpressionValidator { regularExpression: /[a-c]*/ }", this)
          const i = createTemporaryObject(field, this, { inputMask: "AAA", validator: v })
          i.forceActiveFocus()
          keyClick("a"); keyClick("z"); keyClick("b")
          compare(state(i), "ab/ab /2/false")
          keyClick("c")
          compare(state(i), "abc/abc/3/true")
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

  Scenario: Return is taken only when the field is acceptable
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "99" })
          let accepted = 0
          i.accepted.connect(() => accepted++)
          i.forceActiveFocus()
          keyClick(Qt.Key_Return); compare(accepted, 0)
          keyClick("1"); keyClick("2")
          keyClick(Qt.Key_Return); compare(accepted, 1)
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

  Scenario: A hidden field masks every place
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "99:99", echoMode: TextInput.Password, passwordCharacter: "*" })
          compare(i.displayText, "*****")
          compare(i.text, ":")
          i.echoMode = TextInput.NoEcho
          compare(i.displayText, "")
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """

  Scenario: A read-only field keeps its text
    Given the file "tst_mask.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Mask"; when: windowShown; width: 300; height: 100; visible: true
        Component { id: field; TextInput { width: 200; height: 30 } }
        function state(i) { return [i.text, i.displayText, i.cursorPosition, i.acceptableInput].join("/") }
        function test_it() {
          const i = createTemporaryObject(field, this, { inputMask: "99", text: "12", readOnly: true })
          i.forceActiveFocus(); i.cursorPosition = 0
          keyClick("3"); keyClick(Qt.Key_Delete)
          compare(i.text, "12")
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Mask::initTestCase()
      PASS   : Mask::test_it()
      PASS   : Mask::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """
