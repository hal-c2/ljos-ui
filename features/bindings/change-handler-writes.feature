# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html#property-change-signal-handlers
# Verified on Qt 6.12.0 (qml6, offscreen).
# An object's own change handler runs before the bindings that read its
# property (change-handler-order.feature). What it writes meanwhile is a
# change of its own: that change is told inside the handler, and the
# bindings the first change reached, and their handlers, wait for the
# handler to end.

@qt-parity
Feature: What a change handler writes is told inside it, and only that
  A change handler that writes another property hears of that write at once,
  with what the write reaches. The bindings on the property the handler is
  for are brought up to date, and their handlers run, after it ends.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property var table: ({ a: [1], b: [2] })
        property string current: "a"
        readonly property var shown: table[current]
        property int count: 0
        readonly property int twice: count * 2
        property string order: ""
        onCurrentChanged: { order += "starts;"; count++; order += "ends;" }
        onShownChanged: order += "shown " + shown[0] + ";"
        onCountChanged: order += "count " + count + " " + twice + ";"
        onTwiceChanged: order += "twice " + twice + ";"
      }
      """

  Scenario: The handler's own write is told inside it, the first change after it
    When the property "order" of "root" is set to ""
    And the property "current" of "root" is set to "b"
    Then the property "order" of "root" is "starts;count 1 0;twice 2;ends;shown 2;"

  Scenario: The next change is told the same way
    When the property "current" of "root" is set to "b"
    And the property "order" of "root" is set to ""
    And the property "current" of "root" is set to "a"
    Then the property "order" of "root" is "starts;count 2 2;twice 4;ends;shown 1;"
