# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-imports.html#directory-imports
#   https://doc.qt.io/qt-6/qtqml-syntax-directoryimports.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#createQmlObject-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A directory imported by its file URL
  A file may import a directory by its `file:` URL as well as by a path
  relative to itself: a text that is not beside the directory can reach it
  that way. The directory's types are found, and they shadow those of an
  import written before it, as a relative directory import's do. A
  character the URL escapes is part of the directory's name.

  Background:
    Given the file "plugin/Text.qml":
      """qml
      import QtQuick as Q
      Q.Text { color: "red" }
      """
    And the file "odd #1 %25 ?x/Text.qml":
      """qml
      import QtQuick as Q
      Q.Text { color: "red" }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string made: ""
        function make(before, dir, alias, after, sep) {
          const url = Qt.resolvedUrl(dir)
          const named = alias ? " as " + alias : ""
          const lines = [before, 'import "' + url + '"' + named, after]
          const text = lines.filter(l => l !== "").join(sep === ";" ? "; " : "\n")
          try {
            made = "" + Qt.createQmlObject(text, root, "file:///elsewhere/a.qml").color
          } catch (e) {
            made = "" + e
          }
        }
      }
      """

  Scenario Outline: A text that imports a directory by its file URL makes the directory's type
    When the method "make" of "root" is called with "<before>", "<dir>", "<alias>", "<after>", "<sep>"
    Then the property "made" of "root" is "#ff0000"

    Examples:
      | before         | dir                 | alias | after     | sep     |
      | import QtQuick | plugin              |       | Text {}   | newline |
      | import QtQuick | plugin              |       | Text {}   | ;       |
      |                | plugin              |       | Text {}   | newline |
      | import QtQuick | plugin/             |       | Text {}   | newline |
      | import QtQuick | odd %231 %2525 %3Fx |       | Text {}   | newline |
      | import QtQuick | plugin              | P     | P.Text {} | newline |
