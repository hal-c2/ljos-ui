# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-expressions.html
#   https://doc.qt.io/qt-6/qtquick-debugging.html
# Verified on Qt 6.11.2 (qml6, offscreen): both are logged as
# "Scenario.qml:<line>: TypeError: …".

@qt-parity
Feature: instanceof with something that is no type on its right throws, and the throw is logged
  `value instanceof thing` throws a TypeError when the thing cannot say what
  is an instance of it. Like any exception it stops the handler or the
  binding it was thrown in, and is logged with the file and the line.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property var notAType: ({})
        property bool armed: false
        property bool inBinding: armed && root instanceof notAType
        property string events: ""
        signal poke()
        onPoke: { events += "before "; events += (root instanceof notAType); events += "after" }
      }
      """

  Scenario: A handler that throws there stops, and the exception is logged with its line
    When the method "poke" of "root" is called
    Then the property "events" of "root" is "before "
    And the log has a warning matching "Scenario.qml:10: TypeError"

  Scenario: A binding that throws there is logged with its line
    When the property "armed" of "root" is set to true
    Then the log has a warning matching "Scenario.qml:7: TypeError"
