# Sources:
#   docs/ARCHITECTURE.md section 14 (design guardrails)
#   https://doc.qt.io/qt-6/qtqml-modules-qmldir.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#enter-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#transformOrigin-prop
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
#   https://doc.qt.io/qt-6/qml-qtquick-springanimation.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The design module as files an application imports
  `ljos design export` writes the module Ljos.Design as plain QML: the
  palette's colours, the durations and curves of motion, the spacing and
  type scales, and transitions and behaviours that use them. Qt imports
  the same files, so an application looks and moves alike on both.

  Background:
    Given the files of the module Ljos.Design

  Rule: The colours are the palette of the seed, for the scheme in use

    Scenario: A light palette grown from the default hue
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Design
        Item {
          objectName: "root"
          property color canvas: Colors.canvas
          property color accent: Colors.accent
          property color textOnAccent: Colors.textOnAccent
          property bool dark: Colors.dark
        }
        """
      Then the property "canvas" of "root" is "#f8f9fa"
      And the property "accent" of "root" is "#007cb7"
      And the property "textOnAccent" of "root" is "#ffffff"
      And the property "dark" of "root" is false

    Scenario Outline: Another seed hue or scheme picks another palette
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Design
        Item {
          objectName: "root"
          property color accent: Colors.accent
          property color canvas: Colors.canvas
          function pick() { Colors.seedHue = <hue>; Colors.dark = <dark> }
        }
        """
      When the method "pick" of "root" is called
      Then the property "accent" of "root" is "<accent>"
      And the property "canvas" of "root" is "<canvas>"

      Examples:
        | hue | dark  | accent  | canvas  |
        | 30  | false | #bf5546 | #faf9f9 |
        | 20  | false | #bf5546 | #faf9f9 |
        | 240 | true  | #007cb7 | #0c0c0c |

  Rule: Durations, curves and scales are the tokens the checks hold motion to

    Scenario: The numbers of motion, spacing and type
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Design
        Item {
          objectName: "root"
          property int enter: Motion.enterPanel
          property int exit: Motion.exitPanel
          property int frequent: Motion.enterSmallFrequent
          property int points: Motion.easeOut.length
          property real grow: Motion.enterScale
          property int gap: Spacing.md
          property int body: Typography.bodySize
        }
        """
      Then the property "enter" of "root" is 250
      And the property "exit" of "root" is 200
      And the property "frequent" of "root" is 100
      And the property "points" of "root" is 6
      And the property "grow" of "root" is 0.95
      And the property "gap" of "root" is 16
      And the property "body" of "root" is 14

  Rule: A popup comes and goes with the module's transitions

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        import Ljos.Design
        Item {
          id: root; objectName: "root"; width: 400; height: 400
          property bool faint: p.opacity > 0 && p.opacity < 1
          Popup {
            id: p; objectName: "p"; width: 100; height: 100
            enter: EnterTransition { origin: Popup.Top }
            exit: ExitTransition {}
          }
        }
        """

    Scenario: A popup fades in and grows from the side of its anchor
      When the method "open" of "p" is called
      And 50 milliseconds pass
      Then the property "faint" of "root" is true
      And the property "opened" of "p" is false
      And the property "transformOrigin" of "p" is 1

    Scenario: A popup is opened when its entrance is over
      When the method "open" of "p" is called
      And 400 milliseconds pass
      Then the property "opened" of "p" is true
      And the property "opacity" of "p" is 1
      And the property "scale" of "p" is 1

    Scenario: A popup goes quicker than it came
      When the method "open" of "p" is called
      And 400 milliseconds pass
      And the method "close" of "p" is called
      And 300 milliseconds pass
      Then the property "visible" of "p" is false

  Rule: A behaviour animates a change with the module's motion

    Background:
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Design
        Item {
          id: root; objectName: "root"; width: 400; height: 400
          property bool moving: r.x > 0 && r.x < 100
          property bool giving: r.scale > Motion.pressScale && r.scale < 1
          property bool near: Math.abs(s.x - 100) < 1
          Rectangle { id: r; objectName: "r"; width: 10; height: 10; MoveBehavior on x {} PressBehavior on scale {} }
          Rectangle {
            id: s; objectName: "s"; width: 10; height: 10
            Behavior on x { SpringAnimation { spring: Motion.gestureSmallSpring; damping: Motion.gestureSmallDamping; mass: Motion.gestureSmallMass; epsilon: Motion.gestureSmallEpsilon } }
          }
        }
        """

    Scenario: Something that moves takes its time to get there
      When the property "x" of "r" is set to 100
      And 100 milliseconds pass
      Then the property "moving" of "root" is true
      When 300 milliseconds pass
      Then the property "x" of "r" is 100

    Scenario: A press gives quickly
      When the property "scale" of "r" is set to 0.97
      And 30 milliseconds pass
      Then the property "giving" of "root" is true
      When 200 milliseconds pass
      Then the property "scale" of "r" is 0.97

    Scenario: What follows a finger settles on a damped spring
      When the property "x" of "s" is set to 100
      And 2 seconds pass
      Then the property "near" of "root" is true
