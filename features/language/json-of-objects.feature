# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
#   https://tc39.es/ecma262/#sec-json.stringify
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What JSON.stringify writes for an object
  JSON.stringify writes an object with the properties it has, in the order they are
  declared, leaving out its signals and methods and what is undefined or a function. A
  colour, point, rect or vector is written as an object of its parts, a date as its ISO
  text, a url as its text, a list as an array, and an object held by a property as an
  object in turn. A function replacer is called with the object as this and can keep the
  walk out of what leads back; a list of keys as the replacer is not heeded. Walking into
  what it is already inside throws a TypeError; the same object in two places is written
  twice.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string seen
        property QtObject q: QtObject {
          objectName: "q"; property int a: 1; property color c: "red"; property var list: [1, "two", null]
          property list<QtObject> kids: [QtObject { objectName: "k"; property int z: 3 }]
          property QtObject inner: QtObject { property int y: 2 }
          property QtObject none: null; property point p: Qt.point(1, 2); property rect rc: Qt.rect(1, 2, 3, 4)
          property vector3d v: Qt.vector3d(1, 2, 3); property date d: new Date(0); property url u: "http://x/y"
          property real nan: NaN; property var und; property var fn: function () { return 1 }; property int e: Text.AlignRight
        }
        property QtObject deep: QtObject { property int y: 2; property QtObject in2: QtObject { property int z: 1 } }
        function t(f) { try { return String(f()) } catch (e) { return e.name + ": " + e.message } }
        function write() { seen = JSON.stringify(q).replace(/"/g, "'") }
        function replace() {
          const a = {}; a.a = a
          seen = [t(() => JSON.stringify(deep, ["y"])), t(() => JSON.stringify(deep, (k, v) => typeof v === "object" ? v : (k === "y" ? v * 10 : undefined))),
                  t(() => JSON.stringify(deep, function (k, v) { return k === "y" ? (this === deep) + typeof this.y : v })),
                  t(() => JSON.stringify(root, (k, v) => k === "" ? v : undefined)), t(() => JSON.stringify(root, () => undefined)),
                  t(() => JSON.stringify([deep.in2, deep.in2])), t(() => JSON.stringify({ x: 1, y: { x: 2, z: 3 } }, ["y", "x"])),
                  t(() => JSON.stringify(a)), t(() => JSON.stringify(a, (k, v) => v))].join("|").replace(/"/g, "'")
        }
      }
      """

  Scenario: An object's properties are written in their order, each by its type
    When the method "write" of "root" is called
    Then the property "seen" of "root" is "{'objectName':'q','a':1,'c':{'r':1,'g':0,'b':0,'a':1,'hsvHue':0,'hsvSaturation':1,'hsvValue':1,'hslHue':0,'hslSaturation':1,'hslLightness':0.5000076293945312,'valid':true},'list':[1,'two',null],'kids':[{'objectName':'k','z':3}],'inner':{'objectName':'','y':2},'none':null,'p':{'x':1,'y':2},'rc':{'x':1,'y':2,'width':3,'height':4,'left':1,'right':4,'top':2,'bottom':6},'v':{'x':1,'y':2,'z':3},'d':'1970-01-01T00:00:00.000Z','u':'http://x/y','nan':null,'e':2}"

  Scenario: A replacer sees the object, and one that drops every key writes a cyclic item as {}
    When the method "replace" of "root" is called
    Then the property "seen" of "root" is "{'objectName':'','y':2,'in2':{'objectName':'','z':1}}|{'y':20,'in2':{}}|{'objectName':'','y':'truenumber','in2':{'objectName':'','z':1}}|{}|undefined|[{'objectName':'','z':1},{'objectName':'','z':1}]|{'x':1,'y':{'x':2,'z':3}}|TypeError: Cannot convert circular structure to JSON|TypeError: Cannot convert circular structure to JSON"
