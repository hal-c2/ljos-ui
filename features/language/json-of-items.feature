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

  Scenario: An object that leads back to nothing is written, and so is plain data
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string seen
        property QtObject q: QtObject { property int a: 1 }
        Component { id: lone; Item {} }
        Timer { id: tm }
        function t(f) { try { return typeof f() } catch (e) { return e.name } }
        Component.onCompleted: seen = [t(() => JSON.stringify(q)), t(() => JSON.stringify(lone.createObject(null))),
                                       t(() => JSON.stringify(tm)), t(() => JSON.stringify([root])),
                                       JSON.stringify({ a: [1, "b"], c: { d: null } }).replace(/"/g, "'")].join("|")
      }
      """
    Then the property "seen" of "root" is "string|string|TypeError|TypeError|{'a':[1,'b'],'c':{'d':null}}"

