# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-wheelhandler.html#acceptedModifiers-prop
#   https://doc.qt.io/qt-6/qml-qtquick-wheelhandler.html#orientation-prop
#   https://doc.qt.io/qt-6/qml-qtquick-wheelhandler.html#blocking-prop
#   https://doc.qt.io/qt-6/qml-qtquick-wheelhandler.html#enabled-prop
#   https://doc.qt.io/qt-6/qml-qtquick-wheelhandler.html#margin-prop
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity
Feature: Which wheel events a WheelHandler takes, and who else gets them
  A handler takes the events that fit its modifiers and its orientation.
  What it takes stays with it unless it is told not to block; what it does
  not take goes on as if the handler were not there.

  Rule: The events a handler takes

    Scenario: A handler with modifiers takes the wheel only with exactly those held
      Given the file "tst_modifiers.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          WheelHandler { id: handler; acceptedModifiers: Qt.ControlModifier }
          TestCase {
            name: "Modifiers"
            when: windowShown
            function test_modifiers() {
              mouseWheel(box, 10, 10, 0, 120)
              mouseWheel(box, 10, 10, 0, 120, Qt.NoButton, Qt.ControlModifier | Qt.ShiftModifier)
              compare(handler.rotation, 0)
              mouseWheel(box, 10, 10, 0, 120, Qt.NoButton, Qt.ControlModifier)
              compare(handler.rotation, 15)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Modifiers::test_modifiers()
        """

    Scenario: A handler for no modifier leaves the wheel alone while one is held
      Given the file "tst_bare.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          WheelHandler { id: handler; acceptedModifiers: Qt.NoModifier }
          TestCase {
            name: "Bare"
            when: windowShown
            function test_bare() {
              mouseWheel(box, 10, 10, 0, 120, Qt.NoButton, Qt.ShiftModifier)
              compare(handler.rotation, 0)
              mouseWheel(box, 10, 10, 0, 120)
              compare(handler.rotation, 15)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Bare::test_bare()
        """

    Scenario: A vertical handler takes a turn only when it goes up or down at all
      Given the file "tst_across.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          property int turns: 0
          WheelHandler { id: handler; onWheel: box.turns++ }
          TestCase {
            name: "Across"
            when: windowShown
            function test_across() {
              mouseWheel(box, 10, 10, -120, 0)
              compare([box.turns, handler.rotation], [0, 0])
              mouseWheel(box, 10, 10, -240, -60)
              compare([box.turns, handler.rotation], [1, -7.5])
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Across::test_across()
        """

    Scenario: A horizontal handler leaves a turn that only goes up or down alone
      Given the file "tst_along.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          property int turns: 0
          WheelHandler { id: handler; orientation: Qt.Horizontal; onWheel: box.turns++ }
          TestCase {
            name: "Along"
            when: windowShown
            function test_along() {
              mouseWheel(box, 10, 10, 0, -120)
              compare([box.turns, handler.rotation], [0, 0])
              mouseWheel(box, 10, 10, -60, -240)
              compare([box.turns, handler.rotation], [1, -7.5])
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Along::test_along()
        """

    Scenario: While it is active a handler also takes a turn that goes the other way
      Given the file "tst_while.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          property int turns: 0
          WheelHandler { id: handler; activeTimeout: 5; onWheel: box.turns++ }
          TestCase {
            name: "While"
            when: windowShown
            function test_while() {
              mouseWheel(box, 10, 10, 0, 120)
              mouseWheel(box, 10, 10, -120, 0)
              compare([box.turns, handler.rotation], [2, 15])
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : While::test_while()
        """

    Scenario: A handler that is not enabled takes nothing
      Given the file "tst_off.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          WheelHandler { id: handler; enabled: false }
          TestCase {
            name: "Off"
            when: windowShown
            function test_off() {
              mouseWheel(box, 10, 10, 0, 120)
              compare([handler.rotation, handler.active], [0, false])
              handler.enabled = true
              mouseWheel(box, 10, 10, 0, 120)
              compare(handler.rotation, 15)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Off::test_off()
        """

    Scenario: A handler in an item that is not enabled takes nothing
      Given the file "tst_item_off.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          Item {
            id: inner
            anchors.fill: parent
            enabled: false
            WheelHandler { id: handler }
          }
          TestCase {
            name: "ItemOff"
            when: windowShown
            function test_item_off() {
              mouseWheel(box, 10, 10, 0, 120)
              compare(handler.rotation, 0)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : ItemOff::test_item_off()
        """

  Rule: A handler that takes the wheel keeps it from the others

    Scenario: A handler in a Flickable's content keeps the Flickable from scrolling
      Given the file "tst_keeps.qml":
        """qml
        import QtQuick
        import QtTest
        Flickable {
          id: view
          width: 200; height: 200
          contentHeight: 2000
          WheelHandler { id: handler }
          TestCase {
            name: "Keeps"
            when: windowShown
            function test_keeps() {
              mouseWheel(view, 100, 100, 0, -120)
              wait(400)
              compare([handler.rotation, view.contentY], [-15, 0])
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Keeps::test_keeps()
        """

    Scenario: A handler that does not block lets the Flickable scroll too
      Given the file "tst_shares.qml":
        """qml
        import QtQuick
        import QtTest
        Flickable {
          id: view
          width: 200; height: 200
          contentHeight: 2000
          WheelHandler { id: handler; blocking: false }
          TestCase {
            name: "Shares"
            when: windowShown
            function test_shares() {
              mouseWheel(view, 100, 100, 0, -120)
              tryVerify(function() { return view.contentY > 0 })
              compare(handler.rotation, -15)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Shares::test_shares()
        """

    Scenario: A wheel the handler does not take scrolls the Flickable
      Given the file "tst_passes.qml":
        """qml
        import QtQuick
        import QtTest
        Flickable {
          id: view
          width: 200; height: 200
          contentHeight: 2000
          WheelHandler { id: handler; acceptedModifiers: Qt.ControlModifier }
          TestCase {
            name: "Passes"
            when: windowShown
            function test_passes() {
              mouseWheel(view, 100, 100, 0, -120)
              tryVerify(function() { return view.contentY > 0 })
              compare(handler.rotation, 0)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Passes::test_passes()
        """

    Scenario: Refusing the event in the handler's signal changes nothing
      Given the file "tst_refuse.qml":
        """qml
        import QtQuick
        import QtTest
        Flickable {
          id: view
          width: 200; height: 200
          contentHeight: 2000
          WheelHandler { id: handler; onWheel: (event) => event.accepted = false }
          TestCase {
            name: "Refuse"
            when: windowShown
            function test_refuse() {
              mouseWheel(view, 100, 100, 0, -120)
              wait(400)
              compare([handler.rotation, view.contentY], [-15, 0])
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Refuse::test_refuse()
        """

  Rule: The order among handlers and areas

    Scenario: Every handler of an item gets the wheel, the last one first
      Given the file "tst_all.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          property string log
          WheelHandler { onWheel: box.log += "first|" }
          WheelHandler { onWheel: box.log += "second|" }
          TestCase {
            name: "All"
            when: windowShown
            function test_all() {
              mouseWheel(box, 10, 10, 0, 120)
              compare(box.log, "second|first|")
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : All::test_all()
        """

    Scenario: A handler in a MouseArea gets the wheel instead of the area
      Given the file "tst_instead.qml":
        """qml
        import QtQuick
        import QtTest
        MouseArea {
          id: area
          width: 200; height: 200
          property string log
          onWheel: log += "area|"
          WheelHandler { id: handler; onWheel: area.log += "handler|" }
          TestCase {
            name: "Instead"
            when: windowShown
            function test_instead() {
              mouseWheel(area, 10, 10, 0, 120)
              compare(area.log, "handler|")
              handler.blocking = false
              mouseWheel(area, 10, 10, 0, 120)
              compare(area.log, "handler|handler|area|")
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Instead::test_instead()
        """

    Scenario: An area inside the handler's item gets the wheel before the handler
      Given the file "tst_before.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          property string log
          WheelHandler { onWheel: box.log += "handler|" }
          MouseArea {
            id: area
            anchors.fill: parent
            property bool refuse: false
            onWheel: (wheel) => { box.log += "area|"; wheel.accepted = !refuse }
          }
          TestCase {
            name: "Before"
            when: windowShown
            function test_before() {
              mouseWheel(box, 10, 10, 0, 120)
              compare(box.log, "area|")
              area.refuse = true
              mouseWheel(box, 10, 10, 0, 120)
              compare(box.log, "area|area|handler|")
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Before::test_before()
        """

  Rule: The margin

    Scenario: A handler with a margin takes the wheel that far outside its item
      Given the file "tst_margin.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          Item {
            id: box
            x: 50; y: 50; width: 100; height: 100
            WheelHandler { id: handler; margin: 20 }
          }
          TestCase {
            name: "Margin"
            when: windowShown
            function test_margin() {
              mouseWheel(box.parent, 20, 20, 0, 120)
              compare(handler.rotation, 0)
              mouseWheel(box.parent, 40, 100, 0, 120)
              compare(handler.rotation, 15)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Margin::test_margin()
        """

    Scenario: A handler with a margin keeps its item's place among the others
      Given the file "tst_margin_order.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: root; width: 200; height: 200
          property string log
          MouseArea { anchors.fill: parent; onWheel: root.log += "under|" }
          Item {
            x: 50; y: 50; width: 100; height: 100
            WheelHandler { margin: 20; onWheel: root.log += "margin|" }
          }
          Item {
            x: 0; y: 80; width: 45; height: 40
            WheelHandler { onWheel: root.log += "cover|" }
          }
          TestCase {
            name: "MarginOrder"
            when: windowShown
            function test_order() {
              mouseWheel(root, 40, 100, 0, 120)
              compare(root.log, "cover|")
              mouseWheel(root, 40, 60, 0, 120)
              compare(root.log, "cover|margin|")
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : MarginOrder::test_order()
        """
