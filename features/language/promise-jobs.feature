# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-functionlist.html
#   https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise
#   docs/ARCHITECTURE.md (section 7.2, step 3: the JS job queue runs with the timers)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Promise continuations
  What a promise does once it settles (a `then` or `catch` callback, the rest of an `async`
  function) runs once the code that settled it has ended, not inside it: after the handler that
  settled it, and at creation once the objects are complete.

  Scenario: A then callback runs once the handler that queued it ends
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen: "-"
        Component.onCompleted: {
          Promise.resolve("done").then(v => seen = v)
          seen = "waiting"
        }
      }
      """
    When 10 milliseconds pass
    Then the property "seen" of "root" is "done"

  Scenario: Each then of a chain runs
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int value: 0
        Component.onCompleted: Promise.resolve(1).then(v => v + 1).then(v => value = v * 10)
      }
      """
    When 10 milliseconds pass
    Then the property "value" of "root" is 20

  Scenario: A catch callback runs for a promise that is rejected
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen: "-"
        Component.onCompleted: Promise.reject(new Error("no")).catch(e => seen = e.message)
      }
      """
    When 10 milliseconds pass
    Then the property "seen" of "root" is "no"

  Scenario: A promise settled by a Timer continues when the Timer fires
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int ticks: 0
        Timer {
          interval: 50; running: true
          onTriggered: Promise.resolve(1).then(v => root.ticks += v)
        }
      }
      """
    When 30 milliseconds pass
    Then the property "ticks" of "root" is 0
    When 40 milliseconds pass
    Then the property "ticks" of "root" is 1

  # What would wake the application is Ljós's own to tell.
  @ljos-only
  Scenario: Nothing is scheduled once every continuation has run
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int value: 0
        Component.onCompleted: Promise.resolve(3).then(v => value = v)
      }
      """
    When 10 milliseconds pass
    Then the property "value" of "root" is 3
    And the application has nothing scheduled

  # Qt's QML does not parse an async function.
  @ljos-only
  Scenario: Code after an await continues
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen: "-"
        property var twice: async key => (await Promise.resolve(key)) + key
        Component.onCompleted: twice("q").then(v => seen = v)
      }
      """
    When 10 milliseconds pass
    Then the property "seen" of "root" is "qq"
