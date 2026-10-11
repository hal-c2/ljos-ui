# Sources:
#   https://doc.qt.io/qt-6/qml-qttest-testcase.html
#   https://doc.qt.io/qt-6/qml-qttest-toucheventsequence.html
#   https://doc.qt.io/qt-6/qtquicktest-index.html
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity @headless
Feature: Input, time and objects in a TestCase
  A test presses, types and waits as a user would: its mouse, key and
  touch events travel the way real input does, so they find the item that
  is under the pointer or has the keyboard.

  Rule: Mouse events go to the item under the point

    Scenario: A click with no position is at the middle of the item
      Given the file "tst_click.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          MouseArea {
            id: area
            x: 20; y: 30; width: 100; height: 80
            property point at
            onClicked: (mouse) => at = Qt.point(mouse.x, mouse.y)
          }
          TestCase {
            name: "Click"
            when: windowShown
            function test_middle() {
              mouseClick(area)
              compare(area.at, Qt.point(50, 40))
            }
            function test_position() {
              mouseClick(area, 3, 4)
              compare(area.at, Qt.point(3, 4))
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Click::test_middle()
        PASS   : Click::test_position()
        """

    Scenario: An item that lies over another takes the click meant for the one below
      Given the file "tst_cover.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          MouseArea { id: below; anchors.fill: parent; property int clicks: 0; onClicked: clicks += 1 }
          MouseArea {
            id: cover
            width: 100; height: 200
            property int clicks: 0
            onClicked: clicks += 1
          }
          TestCase {
            name: "Cover"
            when: windowShown
            function test_covered() {
              mouseClick(below, 50, 50)
              compare(cover.clicks, 1)
              compare(below.clicks, 0)
              mouseClick(below, 150, 50)
              compare(below.clicks, 1)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Cover::test_covered()
        """

    Scenario: An item that is not enabled gets no click
      Given the file "tst_disabled.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          MouseArea {
            id: area
            anchors.fill: parent
            enabled: false
            property int clicks: 0
            onClicked: clicks += 1
          }
          TestCase {
            name: "Disabled"
            when: windowShown
            function test_click() {
              mouseClick(area)
              compare(area.clicks, 0)
              area.enabled = true
              mouseClick(area)
              compare(area.clicks, 1)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Disabled::test_click()
        """

    Scenario: The button and the modifiers of a click reach the handler
      Given the file "tst_buttons.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          MouseArea {
            id: area
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            property int button: 0
            property int modifiers: 0
            onClicked: (mouse) => { button = mouse.button; modifiers = mouse.modifiers }
          }
          TestCase {
            name: "Buttons"
            when: windowShown
            function test_right_with_shift() {
              mouseClick(area, 10, 10, Qt.RightButton, Qt.ShiftModifier)
              compare(area.button, Qt.RightButton)
              compare(area.modifiers, Qt.ShiftModifier)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Buttons::test_right_with_shift()
        """

    Scenario: A press is held until its release
      Given the file "tst_hold.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          MouseArea { id: area; anchors.fill: parent; hoverEnabled: true }
          TestCase {
            name: "Hold"
            when: windowShown
            function test_press_release() {
              mousePress(area, 10, 10)
              verify(area.pressed)
              mouseMove(area, 30, 40)
              compare(area.mouseX, 30)
              compare(area.mouseY, 40)
              verify(area.pressed)
              mouseRelease(area, 30, 40)
              verify(!area.pressed)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Hold::test_press_release()
        """

    Scenario: A drag presses, moves by the distance given and releases there
      Given the file "tst_drag.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          MouseArea {
            id: area
            anchors.fill: parent
            property var path: []
            onPressed: (mouse) => path = path.concat("press " + mouse.x + "," + mouse.y)
            onPositionChanged: (mouse) => path = path.concat("move " + mouse.x + "," + mouse.y)
            onReleased: (mouse) => path = path.concat("release " + mouse.x + "," + mouse.y)
          }
          TestCase {
            name: "Drag"
            when: windowShown
            function test_drag() {
              mouseDrag(area, 20, 30, 50, -10)
              compare(area.path[0], "press 20,30")
              compare(area.path[area.path.length - 2], "move 70,20")
              compare(area.path[area.path.length - 1], "release 70,20")
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Drag::test_drag()
        """

    Scenario: Two clicks in a row are a double click
      Given the file "tst_double.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          MouseArea {
            id: area
            anchors.fill: parent
            property int clicks: 0
            property int doubles: 0
            onClicked: clicks += 1
            onDoubleClicked: doubles += 1
          }
          TestCase {
            name: "Double"
            when: windowShown
            function test_double() {
              mouseDoubleClickSequence(area)
              compare(area.doubles, 1)
              compare(area.clicks, 1)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Double::test_double()
        """

    Scenario: The wheel scrolls what is under the point
      Given the file "tst_scroll.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          Flickable {
            id: view
            anchors.fill: parent
            contentWidth: 200; contentHeight: 1000
            Rectangle { width: 200; height: 1000; color: "teal" }
          }
          TestCase {
            name: "Scroll"
            when: windowShown
            function test_scroll() {
              compare(view.contentY, 0)
              mouseWheel(view, 100, 100, 0, -120)
              tryVerify(function() { return view.contentY > 0 })
              wait(500)
              var down = view.contentY
              mouseWheel(view, 100, 100, 0, 120)
              tryVerify(function() { return view.contentY < down })
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Scroll::test_scroll()
        """

    Scenario: The wheel turns over the item
      Given the file "tst_wheel.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          MouseArea {
            id: area
            anchors.fill: parent
            property point turned
            onWheel: (wheel) => turned = Qt.point(wheel.angleDelta.x, wheel.angleDelta.y)
          }
          TestCase {
            name: "Wheel"
            when: windowShown
            function test_wheel() {
              mouseWheel(area, 10, 10, 0, 120)
              compare(area.turned, Qt.point(0, 120))
              mouseWheel(area, 10, 10, -240, 0)
              compare(area.turned, Qt.point(-240, 0))
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Wheel::test_wheel()
        """

  Rule: Key events go to the item that has the keyboard

    Scenario: A key click is a press and a release for the focus item
      Given the file "tst_keys.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          Item {
            id: keys
            focus: true
            property var seen: []
            Keys.onPressed: (event) => seen = seen.concat("down " + event.key + " " + event.text)
            Keys.onReleased: (event) => seen = seen.concat("up " + event.key)
          }
          Item { id: idle; property int seen: 0; Keys.onPressed: seen += 1 }
          TestCase {
            name: "Keys"
            when: windowShown
            function test_click() {
              keyClick(Qt.Key_A)
              keyClick("b")
              compare(keys.seen, ["down 65 a", "up 65", "down 66 b", "up 66"])
              compare(idle.seen, 0)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Keys::test_click()
        """

    Scenario: What is typed lands in the text field with the keyboard
      Given the file "tst_typing.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          TextInput { id: field; width: 200; height: 30; focus: true }
          TestCase {
            name: "Typing"
            when: windowShown
            function test_typing() {
              keyClick("h")
              keyClick("i")
              keyClick(Qt.Key_Exclam, Qt.ShiftModifier)
              keyClick(Qt.Key_Backspace)
              compare(field.text, "hi")
              keySequence("Ctrl+A")
              compare(field.selectedText, "hi")
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Typing::test_typing()
        """

    Scenario: A key stays down between keyPress and keyRelease
      Given the file "tst_held.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          Item {
            id: keys
            focus: true
            property bool down: false; property int modifiers: 0
            Keys.onPressed: (event) => {
              if (event.key === Qt.Key_Space) { down = true; modifiers = event.modifiers }
            }
            Keys.onReleased: (event) => { if (event.key === Qt.Key_Space) down = false }
          }
          TestCase {
            name: "Held"
            when: windowShown
            function test_held() {
              keyPress(Qt.Key_Space, Qt.ControlModifier)
              verify(keys.down)
              compare(keys.modifiers, Qt.ControlModifier)
              keyRelease(Qt.Key_Space, Qt.ControlModifier)
              verify(!keys.down)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Held::test_held()
        """

  Rule: A touch is sent when its sequence is committed

    Scenario: A finger that goes down and up is a tap
      Given the file "tst_touch.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          Rectangle {
            id: pad
            width: 100; height: 100
            property int taps: 0
            TapHandler { id: tap; onTapped: pad.taps += 1 }
          }
          TestCase {
            name: "Touch"
            when: windowShown
            function test_tap() {
              const finger = touchEvent(pad)
              finger.press(0, pad, 20, 20)
              verify(!tap.pressed)
              finger.commit()
              verify(tap.pressed)
              finger.release(0, pad, 20, 20).commit()
              verify(!tap.pressed)
              compare(pad.taps, 1)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Touch::test_tap()
        """

  Rule: Waiting lets the application's time pass

    Scenario: A timer fires while the test waits, and not before
      Given the file "tst_wait.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          id: root
          name: "Wait"
          property int fired: 0
          Timer { id: timer; interval: 200; onTriggered: root.fired += 1 }
          function test_wait() {
            timer.start()
            wait(50)
            compare(fired, 0)
            wait(300)
            compare(fired, 1)
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Wait::test_wait()
        """

    Scenario: An animation is over after waiting for its duration
      Given the file "tst_animation.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          Rectangle {
            id: box
            width: 10; height: 10
            NumberAnimation on x { id: slide; running: false; from: 0; to: 100; duration: 200 }
          }
          TestCase {
            name: "Animation"
            when: windowShown
            function test_slide() {
              slide.start()
              tryCompare(box, "x", 100, 2000)
              verify(!slide.running)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Animation::test_slide()
        """

  Rule: Objects made for one test function are gone before the next

    Scenario: A temporary object is destroyed when its function ends
      Given the file "tst_temporary.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          id: root
          name: "Temporary"
          Component { id: maker; Item { objectName: "made"; property int n: 0 } }
          function test_1_made() {
            const made = createTemporaryObject(maker, root, { n: 4 })
            verify(made)
            compare(made.n, 4)
            compare(findChild(root, "made"), made)
          }
          function test_2_gone() {
            compare(findChild(root, "made"), null)
          }
          function test_3_from_text() {
            const made = createTemporaryQmlObject("import QtQuick; Item { width: 12; height: width * 2 }", root)
            compare(made.height, 24)
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Temporary::test_1_made()
        PASS   : Temporary::test_2_gone()
        PASS   : Temporary::test_3_from_text()
        """

  Rule: A grabbed picture of an item can be asked for its pixels

    Scenario: The pixels of a grabbed rectangle have its colour
      Given the file "tst_grab.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 100; height: 100
          Rectangle { id: box; x: 10; y: 10; width: 40; height: 20; color: "#ff0000" }
          TestCase {
            name: "Grab"
            when: windowShown
            function test_pixels() {
              const image = grabImage(box)
              compare(image.width, 40)
              compare(image.height, 20)
              compare(image.red(5, 5), 255)
              compare(image.green(5, 5), 0)
              compare(image.blue(5, 5), 0)
              compare(image.alpha(5, 5), 255)
              compare(image.pixel(39, 19), Qt.rgba(1, 0, 0, 1))
              verify(image.equals(grabImage(box)))
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Grab::test_pixels()
        """
