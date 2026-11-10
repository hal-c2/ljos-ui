# Sources:
#   https://doc.qt.io/qt-6/qguiapplication.html#keyboardModifiers
#   https://doc.qt.io/qt-6/qml-qtquick-mouseevent.html#modifiers-prop
#   https://doc.qt.io/qt-6/qml-qtquick-keyevent.html#modifiers-prop
# Ljós's own: the modifiers the application sees are what native code asks
# (`ljos::keys::keyboard_modifiers`); QML reads `mouse.modifiers` and
# `event.modifiers` in its handlers, and Qt gives QML no application-wide
# value to compare with.

@ljos-only
Feature: The keyboard modifiers the application sees
  The application knows which modifier keys are held without following the
  keys itself. Every key, mouse button, pointer move and wheel event says
  which are held, and the last one to say so is what the application sees,
  so a click with Shift held is seen as Shift even when no key event for
  Shift came: a click made by another program, or Shift pressed before the
  window had the keyboard. A finger on a touch screen and a press by an
  assistive technology say nothing of the keyboard, and leave what the
  application sees as it was.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        width: 200; height: 200
        property string log: ""
        Rectangle {
          width: 200; height: 100
          Accessible.role: Accessible.Button; Accessible.name: "Go"
          MouseArea {
            anchors.fill: parent
            onClicked: (mouse) => root.log += (mouse.modifiers & Qt.ShiftModifier ? "shift " : "plain ")
          }
        }
        Flickable {
          y: 100; width: 200; height: 100
          contentHeight: 400
          Rectangle { width: 200; height: 400 }
        }
      }
      """

  Rule: A pointer event says which modifiers are held

    Scenario: A click with Shift held and no key event for it
      When the user clicks at 50, 50 holding "Shift"
      Then the property "log" of "root" is "shift "
      And the application sees "Shift" held

    Scenario: A click without modifiers after it
      When the user clicks at 50, 50 holding "Ctrl+Shift"
      Then the application sees "Ctrl+Shift" held
      When the user touches 50, 50
      Then the property "log" of "root" is "shift plain "
      And the application sees no modifier held

    Scenario: A drag with Alt held
      When the user drags from 20, 20 to 120, 60 holding "Alt"
      Then the application sees "Alt" held

    Scenario: The wheel turned with Ctrl held
      When the user turns the wheel 1 notch down at 100, 150 holding "Ctrl"
      Then the application sees "Ctrl" held

  Rule: A key event says which modifiers are held

    Scenario: A modifier key kept down and let go of
      When the user keeps "Shift" down
      Then the application sees "Shift" held
      When the user lets go of "Shift"
      Then the application sees no modifier held

    Scenario: Two modifier keys kept down together
      When the user keeps "Ctrl+Shift" down
      Then the application sees "Ctrl+Shift" held
      When the user lets go of "Ctrl+Shift"
      Then the application sees no modifier held

    Scenario: A chord leaves no modifier held once it is let go of
      When the user keeps "Alt+X" down
      Then the application sees "Alt" held
      When the user lets go of "Alt+X"
      Then the application sees no modifier held

  Rule: What says nothing of the keyboard leaves the modifiers as they were

    Scenario: A finger on a touch screen
      When the user keeps "Shift" down
      And the user taps 50, 50 with a finger
      Then the property "log" of "root" is "plain "
      And the application sees "Shift" held

    Scenario: A press by an assistive technology
      Given an assistive technology is listening
      When the user keeps "Meta" down
      And an assistive technology asks the "button" named "Go" to press
      Then the property "log" of "root" is "plain "
      And the application sees "Meta" held
