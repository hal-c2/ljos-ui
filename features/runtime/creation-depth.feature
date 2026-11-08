# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html
#   https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RangeError
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Scripts run however deep the objects nest
  An object deep inside others still runs its script, however many
  Loaders it is made through. Only a script that never stops runs out of
  room, and then it throws a RangeError, which a script can catch.

  Scenario Outline: An object loaded <depth> Loaders deep runs its script
    Given the file "Leaf.qml":
      """qml
      import QtQuick
      Item {
        objectName: "leaf"
        property real made: 0
        function wider(w) { return Math.max(w, 160) }
        Component.onCompleted: made = wider(220)
      }
      """
    And the file "Level.qml":
      """qml
      import QtQuick
      Loader {
        property int depth: 0
        source: depth > 0 ? "Level.qml" : "Leaf.qml"
        onLoaded: if (depth > 0) item.depth = depth - 1
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        Level { depth: <depth> }
      }
      """
    Then the property "made" of "leaf" is 220
    And the log has no warning matching "call stack"

    Examples:
      | depth |
      | 16    |
      | 64    |

  Scenario: A function that calls itself forever throws a RangeError
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string seen: ""
        function down(n) { return down(n + 1) + 1 }
        function run() {
          try { down(0) } catch (e) { seen = e.name }
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "RangeError"
