# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-wheelhandler.html
#   https://doc.qt.io/qt-6/qml-qtquick-wheelevent.html
#   https://doc.qt.io/qt-6/qml-qttest-testcase.html#mouseWheel-method
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity
Feature: WheelHandler turns the wheel into a rotation
  A WheelHandler adds up how far the wheel turned over its parent, in
  degrees, and can add the same amount to a property of a target item.

  Rule: A new handler

    Scenario: A new handler adds up vertical turns and changes no property
      Given the file "tst_new.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          WheelHandler { id: handler }
          TestCase {
            name: "New"
            when: windowShown
            function test_new() {
              compare(handler.rotation, 0)
              compare(handler.rotationScale, 1)
              compare(handler.orientation, Qt.Vertical)
              compare(handler.property, "")
              compare(handler.target, box)
              compare(handler.parent, box)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : New::test_new()
        """

    Scenario: A new handler takes any wheel and keeps it from the others
      Given the file "tst_takes.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          WheelHandler { id: handler }
          TestCase {
            name: "Takes"
            when: windowShown
            function test_takes() {
              compare(handler.enabled, true)
              compare(handler.acceptedModifiers, Qt.KeyboardModifierMask)
              compare(handler.blocking, true)
              compare(handler.active, false)
              compare(handler.activeTimeout, 0.1)
              compare(handler.invertible, true)
              compare(handler.targetTransformAroundCursor, true)
              fuzzyCompare(handler.targetScaleMultiplier, Math.pow(2, 1 / 3), 0.000001)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Takes::test_takes()
        """

  Rule: The handler's rotation

    Scenario: A notch of the wheel is fifteen degrees
      Given the file "tst_notch.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          WheelHandler { id: handler }
          TestCase {
            name: "Notch"
            when: windowShown
            function test_notch() {
              mouseWheel(box, 10, 10, 0, 120)
              compare(handler.rotation, 15)
              mouseWheel(box, 10, 10, 0, 60)
              compare(handler.rotation, 22.5)
              mouseWheel(box, 10, 10, 0, -240)
              compare(handler.rotation, -7.5)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Notch::test_notch()
        """

    Scenario: The rotation scale multiplies what the wheel turned
      Given the file "tst_scale.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          WheelHandler { id: handler; rotationScale: -2 }
          TestCase {
            name: "Scale"
            when: windowShown
            function test_scale() {
              mouseWheel(box, 10, 10, 0, 120)
              compare(handler.rotation, -30)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Scale::test_scale()
        """

    Scenario: A new rotation scale applies to all that the wheel has turned
      Given the file "tst_rescale.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          WheelHandler { id: handler; rotationScale: -2 }
          TestCase {
            name: "Rescale"
            when: windowShown
            function test_rescale() {
              mouseWheel(box, 10, 10, 0, 120)
              handler.rotationScale = 0.1
              fuzzyCompare(handler.rotation, 1.5, 0.000001)
              mouseWheel(box, 10, 10, 0, 120)
              fuzzyCompare(handler.rotation, 3, 0.000001)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Rescale::test_rescale()
        """

    Scenario: A horizontal handler adds up the sideways turns
      Given the file "tst_sideways.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          WheelHandler { id: handler; orientation: Qt.Horizontal }
          TestCase {
            name: "Sideways"
            when: windowShown
            function test_sideways() {
              mouseWheel(box, 10, 10, -120, 0)
              compare(handler.rotation, -15)
              mouseWheel(box, 10, 10, 240, 60)
              compare(handler.rotation, 15)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Sideways::test_sideways()
        """

    Scenario: The next turn goes on from a rotation that was assigned
      Given the file "tst_assigned.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          WheelHandler { id: handler; property: "x" }
          TestCase {
            name: "Assigned"
            when: windowShown
            function test_assigned() {
              handler.rotation = 100
              compare(box.x, 0)
              mouseWheel(box, 10, 10, 0, 120)
              compare(handler.rotation, 115)
              compare(box.x, 15)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Assigned::test_assigned()
        """

  Rule: The wheel signal

    Scenario: The signal carries the event in the parent's coordinates
      Given the file "tst_signal.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          Item {
            id: box
            x: 20; y: 30; width: 100; height: 100
            property var got: []
            WheelHandler {
              onWheel: (event) => box.got = [event.angleDelta.y, event.x, event.y,
                event.buttons, event.modifiers, event.accepted]
            }
          }
          TestCase {
            name: "Signal"
            when: windowShown
            function test_signal() {
              mouseWheel(box.parent, 25, 45, 0, 120, Qt.RightButton, Qt.ShiftModifier)
              compare(box.got, [120, 5, 15, Qt.RightButton, Qt.ShiftModifier, true])
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Signal::test_signal()
        """

    Scenario: The handler is active, then turned, then signals, then moves its target
      Given the file "tst_order.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          property string log
          onXChanged: log += "x " + x + "|"
          WheelHandler {
            id: handler
            property: "x"
            onActiveChanged: box.log += "active " + active + " at " + rotation + "|"
            onRotationChanged: box.log += "rotation " + rotation + "|"
            onWheel: box.log += "wheel at " + box.x + "|"
          }
          TestCase {
            name: "Order"
            when: windowShown
            function test_order() {
              mouseWheel(box, 10, 10, 0, 120)
              compare(box.log, "active true at 0|rotation 15|wheel at 0|x 15|")
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Order::test_order()
        """

    Scenario: The handler stays active for its timeout after the last turn
      Given the file "tst_active.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: box
          width: 200; height: 200
          WheelHandler { id: handler; activeTimeout: 1 }
          TestCase {
            name: "Active"
            when: windowShown
            function test_active() {
              mouseWheel(box, 10, 10, 0, 120)
              compare(handler.active, true)
              wait(600)
              compare(handler.active, true)
              wait(900)
              compare(handler.active, false)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Active::test_active()
        """
