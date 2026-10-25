# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
#   https://tc39.es/ecma262/#sec-json.stringify
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (builtins), buckets throws:TypeError/none:call, value:load:number and
# warning:binding-loop(qt-only):load.

@qt-parity
Feature: JSON of an item
  An item's properties include its parent and its children, which lead back to it, so
  JSON.stringify of an item throws a TypeError for the circular structure, unless a
  replacer keeps the walk out of what leads back.

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

  Scenario: A replacer can keep the walk out of an item's parent and children
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string seen
        Item { id: kid; objectName: "kid"; property int q: 7 }
        function keep(...ks) { return function (k, v) { return k === "" || ks.includes(k) && (k !== "parent" || this === kid) ? v : undefined } }
        Component.onCompleted: seen = [JSON.stringify(kid, keep("objectName", "q")), JSON.stringify(kid, keep("objectName", "parent")),
                                       JSON.stringify(kid, keep("objectName", "parent", "children")),
                                       JSON.stringify(kid, (k, v) => k === "parent" ? (v === root ? "ROOT" : "other") : k === "" || k === "objectName" ? v : undefined)].join("|").replace(/"/g, "'")
      }
      """
    Then the property "seen" of "root" is "{'objectName':'kid','q':7}|{'objectName':'kid','parent':{'objectName':'root'}}|{'objectName':'kid','parent':{'objectName':'root','children':[null]},'children':[]}|{'objectName':'kid','parent':'ROOT'}"
