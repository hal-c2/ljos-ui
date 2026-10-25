# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pointerhandler.html#acceptedDevices-prop
#   https://doc.qt.io/qt-6/qml-qtquick-pointerhandler.html#acceptedModifiers-prop
#   https://doc.qt.io/qt-6/qml-qtquick-pointerhandler.html#acceptedPointerTypes-prop
#   https://doc.qt.io/qt-6/qml-qtquick-handlerpoint.html
# Verified on Qt 6.11.2 (qml6 and qmltestrunner, offscreen).

@qt-parity
Feature: The pointers a pointer handler accepts
  A pointer handler takes a press only from the devices and pointer types
  it accepts, with exactly the modifiers it accepts held: two handlers
  on one item can share it out, the mouse with Ctrl to one and a finger
  to the other. The handler's point tells which device the pointer came
  from and the modifiers that were held.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 200; height: 200
        property string log: ""
        function seen(who, p) {
          log += who + " " + p.device.type + "/" + p.device.pointerType + " " + p.modifiers + "|"
        }
        TapHandler {
          acceptedButtons: Qt.LeftButton
          acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
          acceptedModifiers: Qt.ControlModifier
          onTapped: root.seen("mouse", point)
        }
        TapHandler {
          acceptedDevices: PointerDevice.TouchScreen
          onTapped: root.seen("finger", point)
        }
      }
      """

  @api-QtQuick.TapHandler.acceptedModifiers
  Scenario: A click without the modifier is taken by neither
    When the user touches 50, 50
    Then the property "log" of "root" is ""

  @api-QtQuick.TapHandler.acceptedModifiers @api-QtQuick.HandlerPoint.modifiers
  @api-QtQuick.HandlerPoint.device
  Scenario: A click with Ctrl held is the mouse handler's, once
    When the user clicks at 50, 50 holding "Ctrl"
    Then the property "log" of "root" is "mouse 1/1 67108864|"

  @api-QtQuick.TapHandler.acceptedModifiers
  Scenario: A click with more than the modifier is taken by neither
    When the user clicks at 50, 50 holding "Ctrl+Shift"
    Then the property "log" of "root" is ""

  @api-QtQuick.TapHandler.acceptedDevices @api-QtQuick.HandlerPoint.device
  Scenario: A finger on the screen is the touch handler's, once
    When the user taps 50, 50 with a finger
    Then the property "log" of "root" is "finger 2/2 0|"

  @api-QtQuick.DragHandler.acceptedModifiers
  Scenario: A drag handler drags only with its modifiers held
    Given the file "tst_drag.qml":
      """qml
      import QtQuick
      import QtTest
      Item {
        id: root; width: 300; height: 300
        property string log: ""
        DragHandler {
          target: null; acceptedModifiers: Qt.ControlModifier
          onActiveChanged: root.log += "drag " + active + "|"
        }
        TestCase {
          name: "DragAccepted"
          when: windowShown
          function test_drag() {
            mouseDrag(root, 50, 50, 60, 60)
            mouseDrag(root, 50, 50, 60, 60, Qt.LeftButton, Qt.ControlModifier)
            compare(root.log, "drag true|drag false|")
          }
        }
      }
      """
    When the tests are run
    Then the test report has the lines:
      """
      PASS   : DragAccepted::test_drag()
      """

  @api-QtQuick.PointHandler.acceptedDevices @api-QtQuick.HandlerPoint.device
  Scenario: A point handler for the touch screen follows a finger, not the mouse
    Given the file "tst_point.qml":
      """qml
      import QtQuick
      import QtTest
      Item {
        id: root; width: 300; height: 300
        property string log: ""
        PointHandler {
          id: point; acceptedDevices: PointerDevice.TouchScreen
          onActiveChanged: root.log += "point " + active + "|"
        }
        TestCase {
          name: "PointAccepted"
          when: windowShown
          function test_point() {
            mouseClick(root, 100, 100)
            compare(root.log, "")
            var t = touchEvent(root)
            t.press(0, root, 100, 100).commit()
            compare([point.point.device.type, point.point.device.pointerType],
              [PointerDevice.TouchScreen, PointerDevice.Finger])
            t.release(0, root, 100, 100).commit()
            compare(root.log, "point true|point false|")
          }
        }
      }
      """
    When the tests are run
    Then the test report has the lines:
      """
      PASS   : PointAccepted::test_point()
      """

  @api-QtQuick.HoverHandler.acceptedModifiers
  Scenario: A hover handler hovers only while its modifiers are held
    Given the file "tst_hover.qml":
      """qml
      import QtQuick
      import QtTest
      Item {
        id: root; width: 300; height: 300
        property string log: ""
        HoverHandler {
          acceptedModifiers: Qt.ShiftModifier
          onHoveredChanged: root.log += "hover " + hovered + "|"
        }
        TestCase {
          name: "HoverAccepted"
          when: windowShown
          function test_hover() {
            mouseMove(root, 10, 10)
            compare(root.log, "")
            mouseMove(root, 12, 12, -1, Qt.NoButton, Qt.ShiftModifier)
            mouseMove(root, 14, 14)
            compare(root.log, "hover true|hover false|")
          }
        }
      }
      """
    When the tests are run
    Then the test report has the lines:
      """
      PASS   : HoverAccepted::test_hover()
      """

  @api-QtQuick.WheelHandler.acceptedDevices @api-QtQuick.WheelHandler.acceptedPointerTypes
  @api-QtQuick.WheelHandler.acceptedModifiers
  Scenario: A wheel handler takes a mouse's wheel, with its modifiers
    Given the file "tst_wheel_accepted.qml":
      """qml
      import QtQuick
      import QtTest
      Item {
        id: root
        width: 300; height: 300
        WheelHandler { id: pad; acceptedDevices: PointerDevice.TouchPad }
        WheelHandler { id: plain }
        TestCase {
          name: "WheelAccepted"
          when: windowShown
          function test_wheel() {
            compare(plain.acceptedDevices, PointerDevice.Mouse)
            compare(plain.acceptedPointerTypes, PointerDevice.AllPointerTypes)
            mouseWheel(root, 50, 50, 0, 120)
            compare(pad.rotation, 0)
            compare(plain.rotation, 15)
            plain.acceptedModifiers = Qt.ControlModifier
            mouseWheel(root, 50, 50, 0, 120)
            compare(plain.rotation, 15)
            mouseWheel(root, 50, 50, 0, 120, Qt.NoButton, Qt.ControlModifier)
            compare(plain.rotation, 30)
          }
        }
      }
      """
    When the tests are run
    Then the test report has the lines:
      """
      PASS   : WheelAccepted::test_wheel()
      """
