# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html#property-change-signal-handlers
#   https://doc.qt.io/qt-6/qml-qtquick-animation.html#restart-method
# Verified on Qt 6.12.0 (qml6, offscreen).

@qt-parity
Feature: A change handler that restarts an animation bound to its property
  An object's own change handler runs before the bindings that read the
  property, those on other objects included. An animation whose duration
  binds to the property, restarted by the handler, still has its old
  duration. A handler that wants the new one writes it itself, and that
  write ends the binding: from then on the handler owns the duration.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real seconds: 1
        property real spin: 0
        property real started: -1
        property bool own: false
        onSecondsChanged: {
          if (own)
            turn.duration = seconds * 1000
          turn.restart()
          started = turn.duration
        }
        NumberAnimation on spin { id: turn; from: 0; to: 1000; duration: root.seconds * 1000; running: false }
      }
      """

  Scenario: The handler restarts the animation with the duration it had
    When the property "seconds" of "root" is set to 10
    Then the property "started" of "root" is 1000

  Scenario: A handler that writes the duration restarts with the new one
    When the property "own" of "root" is set to true
    And the property "seconds" of "root" is set to 10
    Then the property "started" of "root" is 10000

  Scenario: Once the handler has written the duration the binding no longer follows
    When the property "own" of "root" is set to true
    And the property "seconds" of "root" is set to 10
    And the property "own" of "root" is set to false
    And the property "seconds" of "root" is set to 2
    Then the property "started" of "root" is 10000
