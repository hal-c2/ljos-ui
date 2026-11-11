# Sources:
#   docs/DIALECT.md §4.7 (input: one pointer stream, gestures as children), §4.14
#   https://doc.qt.io/qt-6/qml-qtquick-draghandler.html
#   https://doc.qt.io/qt-6/qml-qtquick-pointhandler.html
#   https://doc.qt.io/qt-6/qml-qtquick-wheelhandler.html
#   https://doc.qt.io/qt-6/qml-qtquick-drag.html
#   https://doc.qt.io/qt-6/qml-qtquick-droparea.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Ui`'s gestures are Ljós's own names for Qt Quick's input handlers: the scenarios in them
# are `@ljos-only`, and each is held to the numbers of the Qt file before it, which runs on both.
# A drag starts once the pointer is more than 8 pixels from the press; every drag here goes 50.

Feature: Ljós's gestures: Drag, Pointer, Wheel and DragSource
  `Drag` is Qt Quick's DragHandler, `Pointer` its PointHandler and
  `Wheel` its WheelHandler, taking what they accept under the names a
  `Tap` takes it by: `buttons`, `modifiers`, `devices` and
  `pointerTypes`. Qt's attached `Drag`, which carries an item to a
  `DropArea`, is `DragSource`, since `Drag` is the gesture that moves an
  item. A file written with the handlers is lowered member by member.

  Rule: A Drag moves its target as a DragHandler does

    @qt-parity
    Scenario: A DragHandler held to one axis moves its parent along it
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          Rectangle {
            id: box; objectName: "box"
            width: 50; height: 50
            property int starts: 0
            DragHandler {
              yAxis.enabled: false
              acceptedButtons: Qt.LeftButton
              onActiveChanged: if (active) box.starts += 1
            }
          }
        }
        """
      When the user drags from 25, 25 to 75, 65
      Then the property "x" of "box" is 50
      And the property "y" of "box" is 0
      And the property "starts" of "box" is 1

    @ljos-only
    Scenario: A Drag held to one axis moves its parent along it
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          width: 300; height: 200
          Rectangle {
            id: box; objectName: "box"
            width: 50; height: 50
            property int starts: 0
            Drag {
              yAxis.enabled: false
              buttons: Qt.LeftButton
              onActiveChanged: if (active) box.starts += 1
            }
          }
        }
        """
      When the user drags from 25, 25 to 75, 65
      Then the property "x" of "box" is 50
      And the property "y" of "box" is 0
      And the property "starts" of "box" is 1

  Rule: A Pointer follows the one pointer on its item as a PointHandler does

    @qt-parity
    Scenario: A PointHandler tells where the press is
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          width: 200; height: 100
          property real at: -1
          PointHandler {
            acceptedDevices: PointerDevice.Mouse
            onActiveChanged: if (active) root.at = point.position.x
          }
        }
        """
      When the user presses the pointer at 60, 50
      Then the property "at" of "root" is 60

    @ljos-only
    Scenario: A Pointer tells where the press is
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          width: 200; height: 100
          property real at: -1
          Pointer {
            devices: PointerDevice.Mouse
            onActiveChanged: if (active) root.at = point.position.x
          }
        }
        """
      When the user presses the pointer at 60, 50
      Then the property "at" of "root" is 60

  Rule: A Wheel turns a property as a WheelHandler does

    # The Qt backend has no wheel to turn; a test case turns it on both.
    @qt-parity
    Scenario: A WheelHandler adds the degrees turned to a property
      Given the file "tst_handler.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 200; height: 200
          Item {
            id: box
            x: 40; width: 100; height: 100
            WheelHandler { property: "x"; acceptedModifiers: Qt.NoModifier }
          }
          TestCase {
            name: "Wheel"
            when: windowShown
            function test_turn() {
              mouseWheel(box, 10, 10, 0, 120)
              compare(box.x, 55)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Wheel::test_turn()
        """

    @ljos-only
    Scenario: A Wheel adds the degrees turned to a property
      Given the file "tst_wheel.qml":
        """qml
        import Ljos.Ui
        import QtTest
        Item {
          width: 200; height: 200
          Item {
            id: box
            x: 40; width: 100; height: 100
            Wheel { property: "x"; modifiers: Qt.NoModifier }
          }
          TestCase {
            name: "Wheel"
            when: windowShown
            function test_turn() {
              mouseWheel(box, 10, 10, 0, 120)
              compare(box.x, 55)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Wheel::test_turn()
        """

  Rule: A DragSource carries its item to a DropArea as Qt's attached Drag does

    @qt-parity
    Scenario: An item whose attached Drag is made active over a DropArea enters it
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          width: 400; height: 400
          property string log: ""
          function start() { box.Drag.active = true }
          DropArea {
            id: area
            x: 200; y: 200; width: 100; height: 100
            onEntered: root.log += "entered(" + area.drag.x + "," + area.drag.y + ") "
          }
          Rectangle {
            id: box; objectName: "box"
            x: 250; y: 250; width: 20; height: 20
            Drag.hotSpot.x: 10
            Drag.hotSpot.y: 10
            Drag.onActiveChanged: root.log += "active=" + Drag.active + " "
          }
        }
        """
      When the method "start" of "root" is called
      Then the property "log" of "root" is "entered(60,60) active=true "

    @ljos-only
    Scenario: An item whose DragSource is made active over a DropArea enters it
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          width: 400; height: 400
          property string log: ""
          function start() { box.DragSource.active = true }
          DropArea {
            id: area
            x: 200; y: 200; width: 100; height: 100
            onEntered: root.log += "entered(" + area.drag.x + "," + area.drag.y + ") "
          }
          Rectangle {
            id: box; objectName: "box"
            x: 250; y: 250; width: 20; height: 20
            DragSource.hotSpot.x: 10
            DragSource.hotSpot.y: 10
            DragSource.onActiveChanged: root.log += "active=" + DragSource.active + " "
          }
        }
        """
      When the method "start" of "root" is called
      Then the property "log" of "root" is "entered(60,60) active=true "

  Rule: A file with these handlers is lowered to the gestures

    # The area is under the item when its drag starts, so it is entered at once.
    @qt-parity
    Scenario: A DragHandler drags an item whose attached Drag follows it onto a DropArea
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          width: 400; height: 400
          property string log: ""
          DropArea {
            width: 300; height: 100
            onEntered: root.log += "entered "
          }
          Rectangle {
            id: box; objectName: "box"
            width: 50; height: 50
            Drag.active: handler.active
            Drag.hotSpot.x: 25
            Drag.hotSpot.y: 25
            DragHandler { id: handler; objectName: "handler"; yAxis.enabled: false; acceptedButtons: Qt.LeftButton }
          }
        }
        """
      When the user drags from 25, 25 to 225, 25
      Then the property "x" of "box" is 200
      And the property "log" of "root" is "entered "

    @ljos-only
    Scenario: The handlers and the attached Drag lowered act as they did
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          width: 400; height: 400
          property string log: ""
          DropArea {
            width: 300; height: 100
            onEntered: root.log += "entered "
          }
          Rectangle {
            id: box; objectName: "box"
            width: 50; height: 50
            Drag.active: handler.active
            Drag.hotSpot.x: 25
            Drag.hotSpot.y: 25
            DragHandler { id: handler; objectName: "handler"; yAxis.enabled: false; acceptedButtons: Qt.LeftButton }
          }
        }
        """
      Then the property "buttons" of "handler" is 1
      When the user drags from 25, 25 to 225, 25
      Then the property "x" of "box" is 200
      And the property "log" of "root" is "entered "

    @ljos-only @headless
    Scenario: ljos migrate writes the handlers and the attached Drag in Ljós's names
      Given the file "Main.qml":
        """qml
        import QtQuick
        Rectangle {
          id: box
          Drag.active: drag.active; Drag.dragType: Drag.Automatic
          DragHandler { id: drag; acceptedButtons: Qt.LeftButton; snapMode: DragHandler.NoSnap }
          PointHandler { acceptedDevices: PointerDevice.Stylus }
          WheelHandler { property: "rotation"; acceptedModifiers: Qt.ControlModifier }
          Text { text: box.Drag.active ? "away" : "here" }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (12 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        import Ljos.Ui
        Rectangle {
          id: box
          DragSource.active: drag.active; DragSource.dragType: DragSource.Automatic
          Drag { id: drag; buttons: Qt.LeftButton; snapMode: Drag.NoSnap }
          Pointer { devices: PointerDevice.Stylus }
          Wheel { property: "rotation"; modifiers: Qt.ControlModifier }
          Text { text: box.DragSource.active ? "away" : "here" }
        }
        """
