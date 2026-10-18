# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html#signal-parameters
#   https://doc.qt.io/qt-6/qml-qtqml-connections.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A handler that uses a signal's parameters without declaring them is warned of
  A handler written as a block or an expression still sees the signal's
  parameters by their names, but that injection is deprecated: the first
  time such a handler runs, the log says which parameter it used without
  declaring it, where the handler is. It says so once for the handler,
  however often it runs and however many objects have it. A handler that
  declares its parameters, or uses none, is not warned of.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        signal fired(int amount, string why)
        property int got: 0
        property string seen: ""
        onFired: {
          seen += "[" + why + "]"
          got += amount
        }
        Repeater {
          model: 2
          Item { Connections { target: root; function onFired(amount) { root.got += amount } } }
        }
        Item { id: quiet; signal fired(int amount); onFired: root.seen += "quiet" }
        Item { id: never; signal fired(int amount); onFired: root.got = amount }
        Item { id: arrow; signal fired(int amount); onFired: (amount) => root.got += amount }
        function fire() { fired(3, "x"); fired(4, "y") }
        function fireOthers() { quiet.fired(1); arrow.fired(1) }
      }
      """

  Scenario: The handler still sees the parameters
    When the method "fire" of "root" is called
    Then the property "seen" of "root" is "[x][y]"
    And the property "got" of "root" is 21

  Scenario: The first parameter it uses is named, where the handler is
    When the method "fire" of "root" is called
    Then the log has a warning matching "Scenario.qml:7:9 Parameter .why. is not declared. Injection of parameters into signal handlers is deprecated. Use JavaScript functions with formal parameters instead."

  Scenario: The handler is warned of once
    When the method "fire" of "root" is called
    And the method "fire" of "root" is called
    Then the log has 1 warnings matching "Injection of parameters"

  Scenario: Handlers that declare their parameters or use none are not warned of
    When the method "fireOthers" of "root" is called
    Then the log has no warning matching "Injection of parameters"

  Scenario: A handler that has not run is not warned of
    Then the log has no warning matching "Injection of parameters"
