# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-connections.html
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Connections
  A Connections object runs its functions when its target emits the matching signals.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int hits: 0
        property bool listening: true
        property Item source: first
        Item { id: first; objectName: "first"; signal ping(int n) }
        Item { id: second; objectName: "second"; signal ping(int n) }
        Connections {
          target: root.source
          enabled: root.listening
          function onPing(n) { root.hits += n }
        }
        function pingFirst(n) { first.ping(n) }
        function pingSecond(n) { second.ping(n) }
        function useSecond() { source = second }
      }
      """

  Scenario: A signal of the target runs the function with its arguments
    When the method "pingFirst" of "root" is called with 3
    Then the property "hits" of "root" is 3

  Scenario: Signals of other objects are not handled
    When the method "pingSecond" of "root" is called with 3
    Then the property "hits" of "root" is 0

  Scenario: Changing the target moves the connection
    When the method "useSecond" of "root" is called
    And the method "pingFirst" of "root" is called with 1
    And the method "pingSecond" of "root" is called with 5
    Then the property "hits" of "root" is 5

  Scenario: A disabled Connections ignores the signal
    When the property "listening" of "root" is set to false
    And the method "pingFirst" of "root" is called with 3
    Then the property "hits" of "root" is 0

  Scenario: A function for a signal the target does not have is reported
    Given the file "Unknown.qml":
      """qml
      import QtQuick
      Item {
        id: root
        Connections { target: root; function onNothing() {} }
      }
      """
    When "Unknown.qml" is loaded
    Then the log has a warning matching "Detected function .onNothing. in Connections element"

  Scenario: ignoreUnknownSignals silences the report
    Given the file "Quiet.qml":
      """qml
      import QtQuick
      Item {
        id: root
        Connections { target: root; ignoreUnknownSignals: true; function onNothing() {} }
      }
      """
    When "Quiet.qml" is loaded
    Then the log has no warnings

  Scenario: The attached Component of the Connections can be its target
    Given the file "Attached.qml":
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "attached"
        property int completions: 0
        Connections {
          target: Component
          function onCompleted() { root.completions += 1 }
        }
      }
      """
    When "Attached.qml" is loaded
    Then the property "completions" of "attached" is 1
    And the log has no warnings

  Scenario: A function for a signal can also be called as a method
    Given the file "Called.qml":
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "called"
        property string log: ""
        Connections {
          id: c
          target: root
          Component.onCompleted: { onZChanged(); root.log += typeof c.onZChanged }
          function onZChanged() { root.log += "z" }
        }
      }
      """
    When "Called.qml" is loaded
    And the property "z" of "called" is set to 2
    Then the property "log" of "called" is "zfunctionz"
    And the log has no warnings
