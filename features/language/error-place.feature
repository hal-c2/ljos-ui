# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Where an error was made
  An error knows the file and the line it was made at, as lineNumber and
  fileName: the QML file and its line for one made in a binding or a
  handler, the script file's for one made there.

  Scenario: An error is placed where it was made
    Given the file "make.js":
      """
      function make() { return new Error("there") }
      """
    And the QML:
      """qml
      import QtQuick
      import "make.js" as Make
      Item {
        objectName: "root"
        property var bound: new Error("bound")
        property string said
        function place(e) { return e.fileName.replace(/^.*\//, "") + ":" + e.lineNumber }
        function run() {
          var here = new Error("here")
          var thrown
          try { null.x } catch (e) { thrown = e }
          said = [place(bound), place(here), place(thrown), place(Make.make()),
                  Object.keys(here).length].join(" ")
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "said" of "root" is "Scenario.qml:5 Scenario.qml:9 Scenario.qml:11 make.js:2 0"
