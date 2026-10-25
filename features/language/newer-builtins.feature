# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-functionlist.html
#   https://tc39.es/ecma262/#sec-string.prototype.trimstart
#   https://tc39.es/ecma262/#sec-array.prototype.flat
#
# Qt 6.11.2's engine has none of these: each call throws "TypeError: Property 'trimStart' of
# object   a   is not a function" and the like. Ljós runs JavaScript on a newer engine and
# keeps what it has; a script written for Qt does not call them, so nothing that works on Qt
# breaks. Found by ljos-fuzz (builtins), which now calls only what Qt has.

@qt-diverges
Feature: Built-ins newer than Qt's engine
  String trimStart, trimEnd, replaceAll and at, and Array flat and at, work as the standard
  says.

  Scenario: The newer string and array built-ins give what the standard says
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen: ["[" + "  a ".trimStart() + "]", "[" + " a  ".trimEnd() + "]",
          "a-a".replaceAll("a", "b"), "abc".at(-1), [1, [2, [3]]].flat().length, [4, 5].at(-1)].join(",")
      }
      """
    Then the property "seen" of "root" is "[a ],[ a],b-b,c,3,5"
