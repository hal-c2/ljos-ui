# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#callLater-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Qt.callLater
  A call queued with Qt.callLater runs once the current work is done. A call queued while the
  queue drains waits for the next pass of the event loop, so time moves on in between.

  Scenario: A call that queues itself runs once a pass, not without end
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int runs: 0
        property int runsAtTick: -1
        function again() { runs++; if (runsAtTick < 0 && runs < 100000) Qt.callLater(again) }
        Timer { interval: 50; running: true; onTriggered: root.runsAtTick = root.runs }
        Component.onCompleted: Qt.callLater(again)
      }
      """
    When 200 milliseconds pass
    Then the property "runsAtTick" of "root" is at least 2
    And the property "runsAtTick" of "root" is at most 99999

  Scenario: The same function queued twice in one pass runs once
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int runs: 0
        function count() { runs++ }
        Component.onCompleted: { Qt.callLater(count); Qt.callLater(count) }
      }
      """
    When 100 milliseconds pass
    Then the property "runs" of "root" is 1
