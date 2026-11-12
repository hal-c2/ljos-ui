# Sources:
#   https://tc39.es/ecma262/#sec-jobs
#   https://doc.qt.io/qt-6/qtqml-javascript-functionlist.html
# Verified on Qt 6.11.2 (qml6, offscreen), but for the scenario tagged @ljos-only: qml6 does not
# parse `async` functions.

@qt-parity
Feature: Promise jobs
  What a settled promise queues (a `.then` callback, the rest of an `async` function after an
  `await`) runs once the code that queued it is done, without waiting for time to pass. A job a
  job queues runs in the same pass.

  Scenario: A then callback runs after the code that queued it
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string order: ""
        Component.onCompleted: {
          Promise.resolve("job").then(v => order += v)
          order += "code,"
        }
      }
      """
    When 1 milliseconds pass
    Then the property "order" of "root" is "code,job"

  Scenario: A job a job queues runs too
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int depth: 0
        Component.onCompleted: Promise.resolve(1).then(v => v + 1).then(v => v + 1).then(v => depth = v)
      }
      """
    When 1 milliseconds pass
    Then the property "depth" of "root" is 3

  Scenario: A job queued by a timer runs after the timer's handler
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int seen: 0
        Timer { interval: 10; running: true; onTriggered: Promise.resolve(7).then(v => root.seen = v) }
      }
      """
    When 50 milliseconds pass
    Then the property "seen" of "root" is 7

  Scenario: A job that throws does not stop the next one
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string after: "-"
        Component.onCompleted: {
          Promise.resolve().then(() => { throw new Error("boom") })
          Promise.resolve().then(() => after = "ran")
        }
      }
      """
    When 1 milliseconds pass
    Then the property "after" of "root" is "ran"

  # Ljós only: Qt's QML takes no `async` function.
  @ljos-only
  Scenario: An async function goes on after its await
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int got: 0
        property var plusOne: async x => (await x) + 1
        Component.onCompleted: plusOne(Promise.resolve(41)).then(v => got = v)
      }
      """
    When 1 milliseconds pass
    Then the property "got" of "root" is 42
