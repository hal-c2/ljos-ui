# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-imports.html#directory-imports
#   https://doc.qt.io/qt-6/qtqml-syntax-directoryimports.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#createQmlObject-method
#   https://doc.qt.io/qt-6/qtqml-modules-qmldir.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#resolvedUrl-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A directory imported by its file URL
  A file may import a directory by its `file:` URL as well as by a path
  relative to itself: a text that is not beside the directory can reach it
  that way. The directory's types are found, and they shadow those of an
  import written before it, as a relative directory import's do. A
  character the URL escapes is part of the directory's name. A module may
  say where a directory of its own is by a URL it makes from its own, and a
  text that imports that URL makes the directory's types.

  Rule: A text imports a directory by its file URL

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

  Rule: A module says where a directory of its own is

    Background:
      Given the file "Kit/Parts/qmldir":
        """
        module Kit.Parts
        singleton Registry 1.0 Registry.qml
        """
      And the file "Kit/Parts/Registry.qml":
        """qml
        pragma Singleton
        import QtQuick
        QtObject { readonly property string types: Qt.resolvedUrl("plugin") }
        """
      And the file "Kit/Parts/plugin/Text.qml":
        """qml
        import QtQuick as Q
        Q.Text { color: "red" }
        """
      And the QML:
        """qml
        import QtQuick
        import Kit.Parts
        Item {
          id: root
          objectName: "root"
          property string made: ""
          function make() {
            const text = 'import QtQuick; import Kit.Parts; import "' + Registry.types + '"\nText {}'
            try {
              made = "" + Qt.createQmlObject(text, root, "file:///elsewhere/a.qml").color
            } catch (e) {
              made = "" + e
            }
          }
        }
        """

    Scenario: A text imports the URL a module's singleton makes of its directory
      When the method "make" of "root" is called
      Then the property "made" of "root" is "#ff0000"
