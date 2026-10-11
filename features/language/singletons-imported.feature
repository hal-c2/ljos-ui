# Sources:
#   https://doc.qt.io/qt-6/qml-singleton.html
#   https://doc.qt.io/qt-6/qtqml-modules-qmldir.html
#   https://doc.qt.io/qt-6/qtqml-syntax-directoryimports.html
#   https://doc.qt.io/qt-6/qtqml-syntax-imports.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Singleton types of an imported directory or module
  A singleton is seen wherever its directory is: by a file that imports the
  directory by its path, and by a file that imports the module the
  directory's qmldir file names. Behind an import that is given a name, the
  singleton is a member of that name.

  Rule: A directory imported by its path

    Background:
      Given the file "look/qmldir":
        """
        singleton Tints Tints.qml
        """
      And the file "look/Tints.qml":
        """qml
        pragma Singleton
        import QtQuick
        QtObject {
          property color ink: "#112233"
          property int gap: 6
          function doubled(n) { return n * 2 }
          enum Tone { Plain, Loud }
        }
        """

    Scenario: The singleton of a directory the file imports
      Given the QML:
        """qml
        import QtQuick
        import "look"
        Item {
          objectName: "root"
          property color ink: Tints.ink
          property int gap: Tints.gap
          property int doubled: Tints.doubled(gap)
          property int loud: Tints.Loud
        }
        """
      Then the property "ink" of "root" is "#112233"
      And the property "gap" of "root" is 6
      And the property "doubled" of "root" is 12
      And the property "loud" of "root" is 1

    Scenario: Behind a name the import is given
      Given the QML:
        """qml
        import QtQuick
        import "look" as Look
        Item {
          objectName: "root"
          property int gap: Look.Tints.gap
          property int doubled: Look.Tints.doubled(4)
          property int loud: Look.Tints.Loud
          function widen() { Look.Tints.gap = 9 }
        }
        """
      When the method "widen" of "root" is called
      Then the property "gap" of "root" is 9
      And the property "doubled" of "root" is 8
      And the property "loud" of "root" is 1

    Scenario: With and without the name it is one object
      Given the QML:
        """qml
        import QtQuick
        import "look"
        import "look" as Look
        Item {
          objectName: "root"
          property bool same: Tints === Look.Tints
          property int gap: Tints.gap
          function widen() { Look.Tints.gap = 9 }
        }
        """
      When the method "widen" of "root" is called
      Then the property "same" of "root" is true
      And the property "gap" of "root" is 9

  Rule: A module the qmldir file names

    Background:
      Given the file "Kit/qmldir":
        """
        module Kit
        singleton Style 1.0 Style.qml
        Badge 1.0 Badge.qml
        """
      And the file "Kit/Style.qml":
        """qml
        pragma Singleton
        import QtQuick
        QtObject {
          property int pad: 7
          function twice(n) { return n * 2 }
        }
        """
      And the file "Kit/Badge.qml":
        """qml
        import QtQuick
        Item { property int pad: Style.pad }
        """

    Scenario: The singleton of a module the file imports
      Given the QML:
        """qml
        import QtQuick
        import Kit
        Item {
          objectName: "root"
          property int pad: Style.pad
          property int twice: Style.twice(pad)
          Badge { objectName: "badge" }
          function widen() { Style.pad = 11 }
        }
        """
      When the method "widen" of "root" is called
      Then the property "pad" of "root" is 11
      And the property "twice" of "root" is 22
      And the property "pad" of "badge" is 11

    Scenario: Behind a name the module import is given
      Given the QML:
        """qml
        import QtQuick
        import Kit as K
        Item {
          objectName: "root"
          property int pad: K.Style.pad
          property int twice: K.Style.twice(4)
          K.Badge { objectName: "badge" }
        }
        """
      Then the property "pad" of "root" is 7
      And the property "twice" of "root" is 8
      And the property "pad" of "badge" is 7
