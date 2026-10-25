# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html#property-change-signal-handlers
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An object's own change handler runs before the bindings that read the property
  A change handler written on the object itself runs first: the bindings that
  depend on the property are brought up to date after it, so it reads them as
  they were. Further down a chain, the handler of each property sees that
  property new and the bindings that read it as they were. A handler
  connected later, with Connections or connect(), runs after the bindings and
  sees them new. An animation a handler starts from such a binding starts
  where the value was.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property bool open: false
        readonly property int target: open ? 540 : 0
        property real shown: target
        property int chain: shown + 1
        property string seen: ""; property string later: ""
        onOpenChanged: seen += "open " + target + " " + shown + ";"
        onTargetChanged: { seen += "target " + shown + " " + chain + ";"; slide.to = target; slide.restart() }
        NumberAnimation { id: slide; target: box; property: "x"; from: root.shown; duration: 1000 }
        Item { id: box; objectName: "box" }
        onShownChanged: seen += "shown " + shown + " " + chain + ";"
        Connections { target: root; function onTargetChanged() { root.later = root.shown + " " + root.chain } }
      }
      """

  Scenario: Each handler sees the bindings below its property as they were
    When the property "open" of "root" is set to true
    Then the property "seen" of "root" is "open 0 0;target 0 1;shown 540 1;"

  Scenario: The next change starts from the values the last one left
    When the property "open" of "root" is set to true
    And the property "open" of "root" is set to false
    Then the property "seen" of "root" is "open 0 0;target 0 1;shown 540 1;open 540 540;target 540 541;shown 0 541;"

  Scenario: A handler connected with Connections sees the bindings new
    When the property "open" of "root" is set to true
    Then the property "later" of "root" is "540 541"

  Scenario: An animation started by the handler slides from where the value was
    When the property "open" of "root" is set to true
    And 500 milliseconds pass
    Then the property "x" of "box" is within 100 of 270
