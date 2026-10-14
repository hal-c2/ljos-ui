# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#wheel-signal
#   https://doc.qt.io/qt-6/qml-qtquick-wheelevent.html
#   https://doc.qt.io/qt-6/qml-qttest-testcase.html#mouseWheel-method
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity
Feature: The wheel over a MouseArea
  A MouseArea that handles `wheel` is told how the wheel turned. The event
  goes to the topmost item under the pointer that wants it, and an area
  that does not want it lets it go on to what is beneath or around it.

  Rule: The event says what the wheel did

    Scenario: The event tells how far the wheel turned and where
      Given the file "tst_turn.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          MouseArea {
            id: area
            x: 20; y: 30; width: 100; height: 100
            property string got
            onWheel: (wheel) => got = wheel.angleDelta.x + "," + wheel.angleDelta.y
              + " px " + wheel.pixelDelta.x + "," + wheel.pixelDelta.y
              + " at " + wheel.x + "," + wheel.y
          }
          TestCase {
            name: "Turn"
            when: windowShown
            function test_turn() {
              mouseWheel(area, 10, 15, 0, 120)
              compare(area.got, "0,120 px 0,0 at 10,15")
              mouseWheel(area.parent, 40, 40, -240, 60)
              compare(area.got, "-240,60 px 0,0 at 20,10")
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Turn::test_turn()
        """

    Scenario: The deltas are points of whole numbers
      Given the file "tst_whole.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          MouseArea {
            id: area
            anchors.fill: parent
            property string got
            onWheel: (wheel) => got = wheel.angleDelta + " " + wheel.pixelDelta
          }
          TestCase {
            name: "Whole"
            when: windowShown
            function test_whole() {
              mouseWheel(area, 10, 15, 0, 120)
              compare(area.got, "QPoint(0, 120) QPoint(0, 0)")
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Whole::test_whole()
        """

    Scenario: The event tells the buttons and the modifiers that are held
      Given the file "tst_held.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          MouseArea {
            id: area
            anchors.fill: parent
            property var got: []
            onWheel: (wheel) => got = [wheel.buttons, wheel.modifiers]
          }
          TestCase {
            name: "Held"
            when: windowShown
            function test_held() {
              mouseWheel(area, 10, 10, 0, 120)
              compare(area.got, [Qt.NoButton, Qt.NoModifier])
              mouseWheel(area, 10, 10, 0, 120, Qt.RightButton, Qt.ControlModifier | Qt.ShiftModifier)
              compare(area.got, [Qt.RightButton, Qt.ControlModifier | Qt.ShiftModifier])
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Held::test_held()
        """

    Scenario: A wheel of notches is not inverted, has no phase and starts accepted
      Given the file "tst_plain.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          MouseArea {
            id: area
            anchors.fill: parent
            property var got: []
            onWheel: (wheel) => got = [wheel.inverted, wheel.phase, wheel.accepted]
          }
          TestCase {
            name: "Plain"
            when: windowShown
            function test_plain() {
              mouseWheel(area, 10, 10, 0, 120)
              compare(area.got, [false, Qt.NoScrollPhase, true])
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Plain::test_plain()
        """

  Rule: An area that handles the wheel keeps it, unless it says otherwise

    Scenario: An area over a Flickable keeps the wheel from it
      Given the file "tst_over.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          Flickable {
            id: view
            anchors.fill: parent
            contentHeight: 2000
          }
          MouseArea {
            id: area
            anchors.fill: parent
            property int turns: 0
            onWheel: turns++
          }
          TestCase {
            name: "Over"
            when: windowShown
            function test_over() {
              mouseWheel(area, 100, 100, 0, -120)
              wait(400)
              compare([area.turns, view.contentY], [1, 0])
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Over::test_over()
        """

    Scenario: An event that is not accepted goes on to the Flickable beneath
      Given the file "tst_refused.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          Flickable {
            id: view
            anchors.fill: parent
            contentHeight: 2000
          }
          MouseArea {
            id: area
            anchors.fill: parent
            property int turns: 0
            onWheel: (wheel) => { turns++; wheel.accepted = false }
          }
          TestCase {
            name: "Refused"
            when: windowShown
            function test_refused() {
              mouseWheel(area, 100, 100, 0, -120)
              tryVerify(function() { return view.contentY > 0 })
              compare(area.turns, 1)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Refused::test_refused()
        """

    Scenario: An area in a Flickable's content keeps the wheel from the Flickable
      Given the file "tst_inside.qml":
        """qml
        import QtQuick
        import QtTest
        Flickable {
          id: view
          width: 200; height: 200
          contentHeight: 2000
          MouseArea {
            id: area
            width: 200; height: 2000
            property bool refuse: false
            onWheel: (wheel) => wheel.accepted = !refuse
          }
          TestCase {
            name: "Inside"
            when: windowShown
            function test_inside() {
              mouseWheel(view, 100, 100, 0, -120)
              wait(400)
              compare(view.contentY, 0)
              area.refuse = true
              mouseWheel(view, 100, 100, 0, -120)
              tryVerify(function() { return view.contentY > 0 })
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Inside::test_inside()
        """

    Scenario: The area around a refusing area gets an event of its own
      Given the file "tst_nested.qml":
        """qml
        import QtQuick
        import QtTest
        MouseArea {
          id: outer
          width: 200; height: 200
          property string log
          onWheel: (wheel) => log += "outer " + wheel.accepted + " at " + wheel.x + "|"
          MouseArea {
            id: inner
            x: 50; width: 100; height: 100
            property bool refuse: false
            onWheel: (wheel) => { outer.log += "inner|"; wheel.accepted = !refuse }
          }
          TestCase {
            name: "Nested"
            when: windowShown
            function test_nested() {
              mouseWheel(inner, 10, 10, 0, 120)
              compare(outer.log, "inner|")
              inner.refuse = true
              mouseWheel(inner, 10, 10, 0, 120)
              compare(outer.log, "inner|inner|outer true at 60|")
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Nested::test_nested()
        """

  Rule: An area that does not handle the wheel lets it through

    Scenario: The Flickable beneath an area with no wheel handler scrolls
      Given the file "tst_through.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          Flickable {
            id: view
            anchors.fill: parent
            contentHeight: 2000
          }
          MouseArea {
            id: area
            anchors.fill: parent
          }
          TestCase {
            name: "Through"
            when: windowShown
            function test_through() {
              mouseWheel(area, 100, 100, 0, -120)
              tryVerify(function() { return view.contentY > 0 })
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Through::test_through()
        """

    Scenario: The area around an area with no wheel handler gets the wheel
      Given the file "tst_around.qml":
        """qml
        import QtQuick
        import QtTest
        MouseArea {
          id: outer
          width: 200; height: 200
          property int turns: 0
          onWheel: turns++
          MouseArea {
            id: inner
            anchors.fill: parent
          }
          TestCase {
            name: "Around"
            when: windowShown
            function test_around() {
              mouseWheel(inner, 10, 10, 0, 120)
              compare(outer.turns, 1)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Around::test_around()
        """

    Scenario: A disabled area lets the wheel through to the area beneath
      Given the file "tst_disabled.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          property string log
          MouseArea {
            anchors.fill: parent
            onWheel: parent.log += "beneath|"
          }
          MouseArea {
            id: top
            anchors.fill: parent
            enabled: false
            onWheel: parent.log += "top|"
          }
          TestCase {
            name: "Disabled"
            when: windowShown
            function test_disabled() {
              mouseWheel(top, 10, 10, 0, 120)
              compare(top.parent.log, "beneath|")
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Disabled::test_disabled()
        """

  Rule: A Flickable inside an area takes the wheel first

    Scenario: The area gets the wheel once the Flickable inside it is at its end
      Given the file "tst_end.qml":
        """qml
        import QtQuick
        import QtTest
        MouseArea {
          id: area
          width: 200; height: 200
          property int turns: 0
          onWheel: turns++
          Flickable {
            id: view
            anchors.fill: parent
            contentHeight: 2000
          }
          TestCase {
            name: "End"
            when: windowShown
            function test_end() {
              mouseWheel(view, 100, 100, 0, -120)
              tryVerify(function() { return view.contentY > 0 })
              compare(area.turns, 0)
              view.contentY = 1800
              mouseWheel(view, 100, 100, 0, -120)
              compare(area.turns, 1)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : End::test_end()
        """
