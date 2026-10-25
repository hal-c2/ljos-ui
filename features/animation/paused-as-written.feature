# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-animation.html#paused-prop
#   https://doc.qt.io/qt-6/qml-qtquick-animation.html#running-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# Qt gives an object's written values in the order opposite to the one
# they are written in, so `running: true; paused: true` is paused before
# it runs, and only `paused: true; running: true` starts paused.

@qt-parity
Feature: An animation written paused
  `paused` can only pause an animation that is running. One written paused
  that is not running yet runs anyway, and the log says the pause could not
  be used: an animation on a property, one in a Behavior or a Transition,
  and one whose `running: true` is written before `paused: true`. One
  whose `paused: true` is written before `running: true` starts paused.
  The other values written with `running` are all in place when it starts,
  in whatever order they are written.

  @api-QtQuick.Animation.paused
  Scenario: An animation on a property written paused runs
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: r; objectName: "r"
        property bool going: anim.running && !anim.paused
        NumberAnimation on y { id: anim; from: 0; to: 100; duration: 1000; loops: 3; paused: true }
      }
      """
    When 300 milliseconds pass
    Then the property "going" of "r" is true
    And the property "y" of "r" is within 12 of 30
    And the log has a warning matching "setPaused\(\) cannot be used when animation isn't running"

  @api-QtQuick.Animation.paused
  Scenario: An animation written running and then paused runs
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: r; objectName: "r"
        property bool going: anim.running && !anim.paused
        NumberAnimation { id: anim; target: r; property: "y"; from: 0; to: 100; duration: 1000; running: true; paused: true }
      }
      """
    When 300 milliseconds pass
    Then the property "going" of "r" is true
    And the property "y" of "r" is within 12 of 30
    And the log has a warning matching "setPaused\(\) cannot be used when animation isn't running"

  @api-QtQuick.Animation.paused
  Scenario: An animation written paused and then running starts paused
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: r; objectName: "r"; y: 5
        property bool held: anim.running && anim.paused
        NumberAnimation { id: anim; objectName: "anim"; target: r; property: "y"; from: 0; to: 100; duration: 1000; paused: true; running: true }
      }
      """
    When 300 milliseconds pass
    Then the property "held" of "r" is true
    And the property "y" of "r" is 0
    And the log has no warning matching "setPaused"
    When the property "paused" of "anim" is set to false
    And 300 milliseconds pass
    Then the property "y" of "r" is within 12 of 30

  @api-QtQuick.Animation.paused @api-QtQuick.Behavior
  Scenario: An animation of a Behavior written paused runs when the value changes
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: r; objectName: "r"
        Behavior on y { NumberAnimation { duration: 1000; paused: true } }
        Component.onCompleted: y = 100
      }
      """
    When 300 milliseconds pass
    Then the property "y" of "r" is within 12 of 30
    And the log has a warning matching "setPaused\(\) cannot be used when animation isn't running"

  @api-QtQuick.Animation.paused @api-QtQuick.Transition
  Scenario: An animation of a Transition written paused runs when the state changes
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: r; objectName: "r"
        states: State { name: "down"; PropertyChanges { target: r; y: 100 } }
        transitions: Transition { NumberAnimation { property: "y"; duration: 1000; paused: true } }
        Component.onCompleted: state = "down"
      }
      """
    When 300 milliseconds pass
    Then the property "y" of "r" is within 12 of 30
    And the log has a warning matching "setPaused\(\) cannot be used when animation isn't running"

  @api-QtQuick.Animation.running @api-QtQuick.Animation.loops
  Scenario Outline: What is written with running is in place when it starts, in either order
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: r; objectName: "r"
        NumberAnimation { id: anim; objectName: "anim"; target: r; property: "y"; <written>; duration: 100 }
      }
      """
    When 150 milliseconds pass
    Then the property "running" of "anim" is <running>
    And the property "y" of "r" is within 12 of <y>

    Examples:
      | written                                    | running | y  |
      | running: true; loops: 3; from: 0; to: 100  | true    | 50 |
      | from: 0; to: 100; loops: 3; running: true  | true    | 50 |
      | running: true; from: 50; to: 60           | false   | 60 |
      | from: 50; to: 60; running: true           | false   | 60 |

  @api-QtQuick.Animation.alwaysRunToEnd
  Scenario Outline: An animation written to run to its end does, in either order
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: r; objectName: "r"
        NumberAnimation { id: anim; objectName: "anim"; target: r; property: "y"; from: 0; to: 100; duration: 200; <written> }
      }
      """
    When 50 milliseconds pass
    And the method "stop" of "anim" is called
    And 250 milliseconds pass
    Then the property "y" of "r" is 100

    Examples:
      | written                               |
      | running: true; alwaysRunToEnd: true   |
      | alwaysRunToEnd: true; running: true   |
