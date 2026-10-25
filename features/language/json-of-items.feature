# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
#   https://tc39.es/ecma262/#sec-json.stringify
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (builtins), buckets throws:TypeError/none:call, value:load:number and
# warning:binding-loop(qt-only):load.

@qt-parity
Feature: JSON of an item
  An item's properties include its parent and its children, which lead back to it, so
  JSON.stringify of an item throws a TypeError for the circular structure.

  @backlog
  Scenario: JSON.stringify of an item throws
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string seen
        Item { id: kid }
        function t(f) { try { return f() } catch (e) { return e.name + ": " + e.message } }
        Component.onCompleted: seen = t(function () { return JSON.stringify(root) }) + "|" + t(function () { return JSON.stringify(kid) })
      }
      """
    Then the property "seen" of "root" is "TypeError: Cannot convert circular structure to JSON|TypeError: Cannot convert circular structure to JSON"
